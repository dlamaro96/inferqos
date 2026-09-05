//! Work-conserving hierarchical weighted fair scheduler.
//!
//! Requests are grouped by `(class, tenant, application)`. A queue's service credit is the
//! product of configured hierarchy weights divided by its normalized historical service. Deadline
//! pressure and monotonic queue age add bounded boosts. The selected request is charged by its
//! estimated work, so large requests cannot obtain the same service as tiny requests for free.
#![forbid(unsafe_code)]

use inferqos_core::{AdmissionRequest, ServiceClass};
use parking_lot::Mutex;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use thiserror::Error;
use uuid::Uuid;

pub trait Clock: Send + Sync {
    fn now_ns(&self) -> u64;
}

#[derive(Debug)]
pub struct SystemClock {
    epoch: Instant,
}
impl Default for SystemClock {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }
}
impl Clock for SystemClock {
    fn now_ns(&self) -> u64 {
        self.epoch.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }
}

#[derive(Debug, Default)]
pub struct VirtualClock(AtomicU64);
impl VirtualClock {
    pub fn advance(&self, duration: Duration) {
        self.0.fetch_add(
            duration.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::SeqCst,
        );
    }
}
impl Clock for VirtualClock {
    fn now_ns(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub max_depth: usize,
    pub max_bytes: usize,
    pub aging_half_life: Duration,
    pub deadline_window: Duration,
    pub quantum: f64,
}
impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_depth: 10_000,
            max_bytes: 256 * 1024 * 1024,
            aging_half_life: Duration::from_secs(2),
            deadline_window: Duration::from_secs(1),
            quantum: 1_000.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct QueueKey {
    class: ServiceClass,
    tenant: String,
    app: String,
}
struct Queued {
    request: AdmissionRequest,
    enqueued_ns: u64,
    deadline_ns: u64,
    body_bytes: usize,
    sequence: u64,
}
struct QueueState {
    items: VecDeque<Queued>,
    served: f64,
    weight: f64,
}
struct Inner {
    queues: BTreeMap<QueueKey, QueueState>,
    index: HashMap<Uuid, QueueKey>,
    depth: usize,
    bytes: usize,
    sequence: u64,
}

pub struct Scheduler {
    clock: Arc<dyn Clock>,
    config: SchedulerConfig,
    inner: Mutex<Inner>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueSnapshot {
    pub depth: usize,
    pub bytes: usize,
    pub active_queues: usize,
}

impl Scheduler {
    pub fn new(clock: Arc<dyn Clock>, config: SchedulerConfig) -> Self {
        Self {
            clock,
            config,
            inner: Mutex::new(Inner {
                queues: BTreeMap::new(),
                index: HashMap::new(),
                depth: 0,
                bytes: 0,
                sequence: 0,
            }),
        }
    }
    pub fn enqueue(
        &self,
        request: AdmissionRequest,
        body_bytes: usize,
        class_weight: u32,
        tenant_weight: u32,
        app_weight: u32,
    ) -> Result<usize, SchedulerError> {
        let mut state = self.inner.lock();
        if state.depth >= self.config.max_depth {
            return Err(SchedulerError::DepthLimit);
        }
        if state.bytes.saturating_add(body_bytes) > self.config.max_bytes {
            return Err(SchedulerError::ByteLimit);
        }
        if state.index.contains_key(&request.id) {
            return Err(SchedulerError::Duplicate(request.id));
        }
        let request_id = request.id;
        let now = self.clock.now_ns();
        let deadline_ns =
            now.saturating_add(request.deadline.as_nanos().min(u64::MAX as u128) as u64);
        let key = QueueKey {
            class: request.effective_class,
            tenant: request.identity.tenant.clone(),
            app: request.identity.application.clone(),
        };
        state.sequence = state.sequence.wrapping_add(1);
        let sequence = state.sequence;
        let weight = f64::from(class_weight.max(1))
            * f64::from(tenant_weight.max(1))
            * f64::from(app_weight.max(1));
        state
            .queues
            .entry(key.clone())
            .or_insert_with(|| QueueState {
                items: VecDeque::new(),
                served: 0.0,
                weight,
            })
            .items
            .push_back(Queued {
                request,
                enqueued_ns: now,
                deadline_ns,
                body_bytes,
                sequence,
            });
        state.index.insert(request_id, key);
        state.depth += 1;
        state.bytes += body_bytes;
        Ok(state.depth)
    }
    pub fn cancel(&self, id: Uuid) -> bool {
        let mut state = self.inner.lock();
        let Some(key) = state.index.remove(&id) else {
            return false;
        };
        let mut removed_bytes = 0;
        if let Some(queue) = state.queues.get_mut(&key)
            && let Some(pos) = queue.items.iter().position(|q| q.request.id == id)
        {
            removed_bytes = queue.items.remove(pos).map_or(0, |q| q.body_bytes);
        }
        if removed_bytes > 0 {
            state.depth -= 1;
            state.bytes -= removed_bytes;
        }
        if state.queues.get(&key).is_some_and(|q| q.items.is_empty()) {
            state.queues.remove(&key);
        }
        removed_bytes > 0
    }
    pub fn pop_next(&self) -> Option<AdmissionRequest> {
        let now = self.clock.now_ns();
        let mut state = self.inner.lock();
        let aging_ns = self.config.aging_half_life.as_nanos().max(1) as f64;
        let deadline_window_ns = self.config.deadline_window.as_nanos().max(1) as f64;
        let selected = state
            .queues
            .iter()
            .filter_map(|(key, q)| {
                q.items.front().map(|front| {
                    let age = now.saturating_sub(front.enqueued_ns) as f64 / aging_ns;
                    let remaining = front.deadline_ns.saturating_sub(now) as f64;
                    let deadline = if now >= front.deadline_ns {
                        1_000_000.0
                    } else {
                        (deadline_window_ns / remaining.max(1.0)).min(100_000.0)
                    };
                    let fair = (q.weight * self.config.quantum) / (q.served + self.config.quantum);
                    (key.clone(), fair + age + deadline, front.sequence)
                })
            })
            .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.2.cmp(&a.2)))?
            .0;
        let queue = state
            .queues
            .get_mut(&selected)
            .expect("selected queue exists");
        let item = queue.items.pop_front().expect("selected queue is nonempty");
        queue.served += item.request.estimate.normalized_units.0.max(1.0) / queue.weight.max(1.0);
        let min_served = state
            .queues
            .values()
            .map(|q| q.served)
            .fold(f64::INFINITY, f64::min);
        if min_served.is_finite() && min_served > self.config.quantum * 1000.0 {
            for q in state.queues.values_mut() {
                q.served -= min_served;
            }
        }
        state.index.remove(&item.request.id);
        state.depth -= 1;
        state.bytes -= item.body_bytes;
        if state
            .queues
            .get(&selected)
            .is_some_and(|q| q.items.is_empty())
        {
            state.queues.remove(&selected);
        }
        Some(item.request)
    }
    pub fn snapshot(&self) -> QueueSnapshot {
        let s = self.inner.lock();
        QueueSnapshot {
            depth: s.depth,
            bytes: s.bytes,
            active_queues: s.queues.len(),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum SchedulerError {
    #[error("queue depth limit reached")]
    DepthLimit,
    #[error("queue byte limit reached")]
    ByteLimit,
    #[error("request {0} is already queued")]
    Duplicate(Uuid),
}

#[cfg(test)]
mod tests {
    use super::*;
    use inferqos_core::{EstimateSource, IdentityContext, WorkEstimate, WorkUnits};
    fn request(
        clock: &VirtualClock,
        tenant: &str,
        class: ServiceClass,
        work: f64,
        deadline: Duration,
    ) -> AdmissionRequest {
        let _ = clock;
        AdmissionRequest {
            id: Uuid::new_v4(),
            identity: IdentityContext {
                principal: tenant.into(),
                tenant: tenant.into(),
                application: "app".into(),
                trusted: true,
            },
            requested_class: class,
            effective_class: class,
            pool: "p".into(),
            estimate: WorkEstimate {
                input_tokens: 1,
                output_tokens: 1,
                cached_input_tokens: 0,
                provider_cost_coefficient: 1.0,
                normalized_units: WorkUnits(work),
                confidence: 1.0,
                source: EstimateSource::ExactTokenizer,
            },
            deadline,
            queueable: true,
        }
    }
    #[test]
    fn unused_share_is_borrowed() {
        let clock = Arc::new(VirtualClock::default());
        let s = Scheduler::new(clock.clone(), SchedulerConfig::default());
        for _ in 0..50 {
            s.enqueue(
                request(
                    &clock,
                    "batch",
                    ServiceClass::Batch,
                    10.0,
                    Duration::from_secs(60),
                ),
                1,
                1,
                1,
                1,
            )
            .unwrap();
        }
        for _ in 0..50 {
            assert!(s.pop_next().is_some());
        }
        assert_eq!(s.snapshot().depth, 0);
    }
    #[test]
    fn urgent_deadline_wins() {
        let clock = Arc::new(VirtualClock::default());
        let s = Scheduler::new(clock.clone(), SchedulerConfig::default());
        s.enqueue(
            request(
                &clock,
                "a",
                ServiceClass::Batch,
                1.0,
                Duration::from_secs(60),
            ),
            1,
            1,
            1,
            1,
        )
        .unwrap();
        let urgent = request(
            &clock,
            "b",
            ServiceClass::Interactive,
            1.0,
            Duration::from_millis(1),
        );
        let id = urgent.id;
        s.enqueue(urgent, 1, 1, 1, 1).unwrap();
        assert_eq!(s.pop_next().unwrap().id, id);
    }
    #[test]
    fn cancellation_frees_all_accounting() {
        let clock = Arc::new(VirtualClock::default());
        let s = Scheduler::new(clock.clone(), SchedulerConfig::default());
        let r = request(
            &clock,
            "a",
            ServiceClass::Standard,
            1.0,
            Duration::from_secs(1),
        );
        let id = r.id;
        s.enqueue(r, 99, 1, 1, 1).unwrap();
        assert!(s.cancel(id));
        assert_eq!(s.snapshot().bytes, 0);
    }
    proptest::proptest! { #[test] fn depth_never_exceeds_bound(n in 0usize..500) { let clock=Arc::new(VirtualClock::default()); let s=Scheduler::new(clock.clone(), SchedulerConfig { max_depth:100,..Default::default() }); let mut accepted=0; for _ in 0..n { if s.enqueue(request(&clock,"t",ServiceClass::Standard,1.0,Duration::from_secs(1)),1,1,1,1).is_ok(){accepted+=1;} } proptest::prop_assert_eq!(s.snapshot().depth,accepted.min(100)); } }
}
