//! Capacity reservation coordinators for standalone and HA operation.
#![forbid(unsafe_code)]
use async_trait::async_trait;
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Lease {
    pub id: Uuid,
    pub pool: String,
    pub units: f64,
    pub expires_at: Instant,
}

#[async_trait]
pub trait Coordinator: Send + Sync {
    async fn reserve(
        &self,
        pool: &str,
        limit: f64,
        units: f64,
        ttl: Duration,
    ) -> Result<Option<Lease>, CoordinatorError>;
    async fn release(&self, lease: &Lease) -> Result<(), CoordinatorError>;
    async fn renew(&self, lease: &Lease, ttl: Duration) -> Result<bool, CoordinatorError>;
    async fn healthy(&self) -> Result<(), CoordinatorError>;
}

type PoolLeases = HashMap<Uuid, (f64, Instant)>;
type LeaseState = HashMap<String, PoolLeases>;

#[derive(Default)]
pub struct InMemoryCoordinator {
    state: Mutex<LeaseState>,
}
#[async_trait]
impl Coordinator for InMemoryCoordinator {
    async fn reserve(
        &self,
        pool: &str,
        limit: f64,
        units: f64,
        ttl: Duration,
    ) -> Result<Option<Lease>, CoordinatorError> {
        if !units.is_finite() || units < 0.0 {
            return Err(CoordinatorError::InvalidUnits);
        }
        let now = Instant::now();
        let mut all = self.state.lock();
        let leases = all.entry(pool.into()).or_default();
        leases.retain(|_, (_, expires)| *expires > now);
        let used: f64 = leases.values().map(|(u, _)| u).sum();
        if used + units > limit {
            return Ok(None);
        }
        let lease = Lease {
            id: Uuid::new_v4(),
            pool: pool.into(),
            units,
            expires_at: now + ttl,
        };
        leases.insert(lease.id, (units, lease.expires_at));
        Ok(Some(lease))
    }
    async fn release(&self, lease: &Lease) -> Result<(), CoordinatorError> {
        if let Some(pool) = self.state.lock().get_mut(&lease.pool) {
            pool.remove(&lease.id);
        }
        Ok(())
    }
    async fn renew(&self, lease: &Lease, ttl: Duration) -> Result<bool, CoordinatorError> {
        let mut state = self.state.lock();
        let Some(pool) = state.get_mut(&lease.pool) else {
            return Ok(false);
        };
        let Some((_, expiry)) = pool.get_mut(&lease.id) else {
            return Ok(false);
        };
        *expiry = Instant::now() + ttl;
        Ok(true)
    }
    async fn healthy(&self) -> Result<(), CoordinatorError> {
        Ok(())
    }
}

pub struct ValkeyCoordinator {
    manager: redis::aio::ConnectionManager,
    namespace: String,
}
impl ValkeyCoordinator {
    pub async fn connect(
        url: &str,
        namespace: impl Into<String>,
    ) -> Result<Self, CoordinatorError> {
        let client = redis::Client::open(url).map_err(CoordinatorError::Redis)?;
        let manager = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(CoordinatorError::Redis)?;
        Ok(Self {
            manager,
            namespace: namespace.into(),
        })
    }
    fn key(&self, pool: &str) -> String {
        format!("{}:leases:{}", self.namespace, pool)
    }
}
const RESERVE_SCRIPT: &str = r#"
local key=KEYS[1]
local now=tonumber(ARGV[1])
local limit=tonumber(ARGV[2])
local units=tonumber(ARGV[3])
local lease=ARGV[4]
local expiry=tonumber(ARGV[5])
local entries=redis.call('HGETALL',key)
local used=0
for i=1,#entries,2 do
  local sep=string.find(entries[i+1],':')
  local amount=tonumber(string.sub(entries[i+1],1,sep-1))
  local expires=tonumber(string.sub(entries[i+1],sep+1))
  if expires<=now then redis.call('HDEL',key,entries[i]) else used=used+amount end
end
if used+units>limit then return 0 end
redis.call('HSET',key,lease,tostring(units)..':'..tostring(expiry))
redis.call('PEXPIRE',key,math.max(1000,expiry-now+1000))
return 1
"#;
#[async_trait]
impl Coordinator for ValkeyCoordinator {
    async fn reserve(
        &self,
        pool: &str,
        limit: f64,
        units: f64,
        ttl: Duration,
    ) -> Result<Option<Lease>, CoordinatorError> {
        if !units.is_finite() || units < 0.0 {
            return Err(CoordinatorError::InvalidUnits);
        }
        let id = Uuid::new_v4();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CoordinatorError::Clock)?
            .as_millis() as u64;
        let ttl_ms = ttl.as_millis().min(u64::MAX as u128) as u64;
        let mut conn = self.manager.clone();
        let accepted: i32 = redis::Script::new(RESERVE_SCRIPT)
            .key(self.key(pool))
            .arg(now)
            .arg(limit)
            .arg(units)
            .arg(id.to_string())
            .arg(now.saturating_add(ttl_ms))
            .invoke_async(&mut conn)
            .await
            .map_err(CoordinatorError::Redis)?;
        Ok((accepted == 1).then(|| Lease {
            id,
            pool: pool.into(),
            units,
            expires_at: Instant::now() + ttl,
        }))
    }
    async fn release(&self, lease: &Lease) -> Result<(), CoordinatorError> {
        let mut conn = self.manager.clone();
        let _: usize = redis::cmd("HDEL")
            .arg(self.key(&lease.pool))
            .arg(lease.id.to_string())
            .query_async(&mut conn)
            .await
            .map_err(CoordinatorError::Redis)?;
        Ok(())
    }
    async fn renew(&self, lease: &Lease, ttl: Duration) -> Result<bool, CoordinatorError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| CoordinatorError::Clock)?
            .as_millis() as u64;
        let ttl_ms = ttl.as_millis().min(u64::MAX as u128) as u64;
        let script = redis::Script::new(
            r#"local value=redis.call('HGET',KEYS[1],ARGV[1]); if not value then return 0 end; local sep=string.find(value,':'); local amount=string.sub(value,1,sep-1); redis.call('HSET',KEYS[1],ARGV[1],amount..':'..ARGV[2]); redis.call('PEXPIRE',KEYS[1],ARGV[3]); return 1"#,
        );
        let mut connection = self.manager.clone();
        let renewed: i32 = script
            .key(self.key(&lease.pool))
            .arg(lease.id.to_string())
            .arg(now.saturating_add(ttl_ms))
            .arg(ttl_ms.saturating_add(1000))
            .invoke_async(&mut connection)
            .await
            .map_err(CoordinatorError::Redis)?;
        Ok(renewed == 1)
    }
    async fn healthy(&self) -> Result<(), CoordinatorError> {
        let mut conn = self.manager.clone();
        let pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(CoordinatorError::Redis)?;
        if pong == "PONG" {
            Ok(())
        } else {
            Err(CoordinatorError::Unhealthy(pong))
        }
    }
}

#[derive(Debug, Error)]
pub enum CoordinatorError {
    #[error("coordinator rejected invalid work units")]
    InvalidUnits,
    #[error("system clock precedes Unix epoch")]
    Clock,
    #[error("Valkey operation failed: {0}")]
    Redis(redis::RedisError),
    #[error("coordinator is unhealthy: {0}")]
    Unhealthy(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn expiry_recovers_capacity() {
        let c = InMemoryCoordinator::default();
        let a = c
            .reserve("p", 10.0, 10.0, Duration::from_millis(1))
            .await
            .unwrap();
        assert!(a.is_some());
        tokio::time::sleep(Duration::from_millis(3)).await;
        assert!(
            c.reserve("p", 10.0, 10.0, Duration::from_secs(1))
                .await
                .unwrap()
                .is_some()
        );
    }
    #[tokio::test]
    async fn release_is_idempotent() {
        let c = InMemoryCoordinator::default();
        let l = c
            .reserve("p", 1.0, 1.0, Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        c.release(&l).await.unwrap();
        c.release(&l).await.unwrap();
    }
    #[tokio::test]
    async fn renewal_extends_lease() {
        let c = InMemoryCoordinator::default();
        let lease = c
            .reserve("p", 1.0, 1.0, Duration::from_millis(2))
            .await
            .unwrap()
            .unwrap();
        assert!(c.renew(&lease, Duration::from_secs(1)).await.unwrap());
        tokio::time::sleep(Duration::from_millis(3)).await;
        assert!(
            c.reserve("p", 1.0, 1.0, Duration::from_secs(1))
                .await
                .unwrap()
                .is_none()
        );
    }
}
