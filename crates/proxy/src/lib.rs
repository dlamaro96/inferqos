//! Transparent streaming proxy and separately bound management plane.
#![forbid(unsafe_code)]
use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{any, get},
};
use futures_util::stream;
use inferqos_capacity::CapacityModel;
use inferqos_config::{ApplicationPolicy, Config, ExternalAdapterConfig, Mode, ProviderKind};
use inferqos_coordinator::{Coordinator, InMemoryCoordinator, Lease, ValkeyCoordinator};
use inferqos_core::{
    AdmissionRequest, CoreError, HEADER_CLASS, HEADER_DEADLINE_MS, HEADER_QUEUEABLE,
    HEADER_REQUEST_ID, IdentityContext, ProviderAdapter, ProxyRequest, ServiceClass, WorkEstimate,
};
use inferqos_identity::{IdentityResolver, VerifiedClientCertificate};
use inferqos_providers::HttpProvider;
use inferqos_scheduler::{Scheduler, SchedulerConfig, SystemClock};
use inferqos_telemetry::RuntimeMetrics;
use parking_lot::Mutex;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::io::AsyncWriteExt;
use tokio::sync::{RwLock, oneshot};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState(Arc<Inner>);
struct PoolRuntime {
    provider: Arc<dyn ProviderAdapter>,
    capacity: Arc<CapacityModel>,
    limit: f64,
}
struct Pending {
    sender: oneshot::Sender<AdmissionLeases>,
    request: AdmissionRequest,
    body_bytes: usize,
    class_weight: u32,
    tenant_weight: u32,
    app_weight: u32,
    queued_at: Instant,
}
#[derive(Debug)]
struct AdmissionLeases {
    capacity: Lease,
    tenant_slot: Lease,
    application_slot: Lease,
}

/// Owns every claim created by admission. If the request future is cancelled
/// before response streaming takes ownership, Drop releases local bookkeeping
/// immediately and schedules coordinator settlement. Lease expiry remains the
/// final crash-recovery mechanism.
struct ActiveAdmissionGuard {
    state: AppState,
    capacity: Arc<CapacityModel>,
    leases: Option<AdmissionLeases>,
    identity: IdentityContext,
    estimate: WorkEstimate,
    effective_class: String,
    concurrency_entered: bool,
    counted_active: bool,
}

impl ActiveAdmissionGuard {
    async fn complete(mut self, actual: Option<WorkEstimate>, throttled: bool) {
        if let Some(leases) = self.leases.take() {
            self.state.release_distributed(&leases).await;
            let actual_units = actual.as_ref().map(|value| value.normalized_units);
            self.capacity.release(
                leases.capacity.id,
                self.estimate.normalized_units,
                actual_units,
                throttled,
            );
            if let Some(actual) = actual {
                self.state.0.otel.reconciliation(
                    self.estimate.normalized_units.0,
                    actual.normalized_units.0,
                    &self.effective_class,
                );
            }
        }
        self.release_local();
    }

    fn release_local(&mut self) {
        if self.concurrency_entered {
            self.state.leave(&self.identity);
            self.concurrency_entered = false;
        }
        if self.counted_active {
            self.state.0.metrics.active.fetch_sub(1, Ordering::Relaxed);
            self.state.0.otel.active(-1, &self.effective_class);
            self.counted_active = false;
        }
    }
}

impl Drop for ActiveAdmissionGuard {
    fn drop(&mut self) {
        if let Some(leases) = self.leases.take() {
            self.capacity.release(
                leases.capacity.id,
                self.estimate.normalized_units,
                None,
                false,
            );
            let state = self.state.clone();
            tokio::spawn(async move { state.release_distributed(&leases).await });
        }
        self.release_local();
    }
}
struct Inner {
    config: Arc<RwLock<Config>>,
    pools: HashMap<String, PoolRuntime>,
    scheduler: Arc<Scheduler>,
    pending: Mutex<HashMap<Uuid, Pending>>,
    decisions: Mutex<VecDeque<DecisionRecord>>,
    draining: AtomicBool,
    metrics: Metrics,
    coordinator: Arc<dyn Coordinator>,
    lease_ttl: Duration,
    identity: IdentityResolver,
    concurrency: Mutex<ConcurrencyState>,
    otel: RuntimeMetrics,
}

#[derive(Default)]
struct ConcurrencyState {
    tenants: HashMap<String, usize>,
    applications: HashMap<String, usize>,
}

enum StoredBody {
    Memory(bytes::Bytes),
    Spool {
        path: tempfile::TempPath,
        len: usize,
    },
}

impl StoredBody {
    async fn store(
        body: bytes::Bytes,
        threshold: usize,
        directory: &Path,
    ) -> Result<Self, std::io::Error> {
        if body.len() <= threshold {
            return Ok(Self::Memory(body));
        }
        prepare_spool_directory(directory).await?;
        let file = tempfile::Builder::new()
            .prefix("inferqos-")
            .suffix(".body")
            .tempfile_in(directory)?;
        let (file, path) = file.into_parts();
        let mut file = tokio::fs::File::from_std(file);
        file.write_all(&body).await?;
        file.flush().await?;
        drop(file);
        Ok(Self::Spool {
            path,
            len: body.len(),
        })
    }

    fn len(&self) -> usize {
        match self {
            Self::Memory(body) => body.len(),
            Self::Spool { len, .. } => *len,
        }
    }

    async fn load(&self) -> Result<bytes::Bytes, std::io::Error> {
        match self {
            Self::Memory(body) => Ok(body.clone()),
            Self::Spool { path, len } => {
                let body = tokio::fs::read(path).await?;
                if body.len() != *len {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "spooled body length changed",
                    ));
                }
                Ok(body.into())
            }
        }
    }
}

async fn prepare_spool_directory(directory: &Path) -> Result<(), std::io::Error> {
    tokio::fs::create_dir_all(directory).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).await?;
    }
    Ok(())
}
#[derive(Default)]
struct Metrics {
    requests: AtomicU64,
    admitted: AtomicU64,
    queued: AtomicU64,
    rejected: AtomicU64,
    shadow_would_queue: AtomicU64,
    provider_throttles: AtomicU64,
    active: AtomicU64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DecisionRecord {
    pub request_id: Uuid,
    pub effective_class: String,
    pub tenant: String,
    pub application: String,
    pub pool: String,
    pub estimated_work_units: f64,
    pub outcome: String,
    pub queue_age_ms: u64,
}

impl AppState {
    pub async fn build(config: Config) -> Result<Self, CoreError> {
        let identity = IdentityResolver::new(config.clone())
            .await
            .map_err(|error| {
                CoreError::Provider(format!("identity initialization failed: {error}"))
            })?;
        let (coordinator, lease_ttl): (Arc<dyn Coordinator>, Duration) = match &config.coordinator {
            inferqos_config::CoordinatorConfig::Memory => (
                Arc::new(InMemoryCoordinator::default()),
                Duration::from_secs(300),
            ),
            inferqos_config::CoordinatorConfig::Valkey { url_env, lease_ttl } => {
                let url = std::env::var(url_env).map_err(|_| {
                    CoreError::Provider(format!(
                        "coordinator environment variable {url_env} is not set"
                    ))
                })?;
                let value = ValkeyCoordinator::connect(&url, "inferqos:v2")
                    .await
                    .map_err(|error| CoreError::Provider(error.to_string()))?;
                value
                    .healthy()
                    .await
                    .map_err(|error| CoreError::Provider(error.to_string()))?;
                (Arc::new(value), *lease_ttl)
            }
        };
        let mut pools = HashMap::new();
        for (name, pool) in &config.pools {
            let provider: Arc<dyn ProviderAdapter> =
                if matches!(pool.provider, ProviderKind::ExternalGrpc) {
                    let endpoint = match pool
                        .external_adapter
                        .as_ref()
                        .expect("validated external adapter")
                    {
                        ExternalAdapterConfig::Unix { path } => {
                            inferqos_provider_protocol::AdapterEndpoint::Unix { path: path.clone() }
                        }
                        ExternalAdapterConfig::Loopback { uri } => {
                            inferqos_provider_protocol::AdapterEndpoint::Loopback {
                                uri: uri.clone(),
                            }
                        }
                        ExternalAdapterConfig::Tls {
                            uri,
                            domain,
                            ca_file,
                            client_cert_file,
                            client_key_file,
                        } => inferqos_provider_protocol::AdapterEndpoint::Tls {
                            uri: uri.clone(),
                            domain: domain.clone(),
                            ca_pem: ca_file.clone(),
                            client_cert_pem: client_cert_file.clone(),
                            client_key_pem: client_key_file.clone(),
                        },
                    };
                    Arc::new(
                        inferqos_provider_protocol::ExternalProviderClient::connect(endpoint)
                            .await?,
                    )
                } else {
                    Arc::new(HttpProvider::from_config(pool).await?)
                };
            let capacity = Arc::new(CapacityModel::new(
                pool.capacity_units,
                pool.initial_safety_factor,
            ));
            pools.insert(
                name.clone(),
                PoolRuntime {
                    provider,
                    capacity,
                    limit: pool.capacity_units,
                },
            );
        }
        let scheduler = Arc::new(Scheduler::new(
            Arc::new(SystemClock::default()),
            SchedulerConfig {
                max_depth: config.limits.total_queue_depth,
                max_bytes: config.limits.total_queue_bytes,
                ..Default::default()
            },
        ));
        let state = Self(Arc::new(Inner {
            config: Arc::new(RwLock::new(config)),
            pools,
            scheduler,
            pending: Mutex::new(HashMap::new()),
            decisions: Mutex::new(VecDeque::new()),
            draining: AtomicBool::new(false),
            metrics: Metrics::default(),
            coordinator,
            lease_ttl,
            identity,
            concurrency: Mutex::new(ConcurrencyState::default()),
            otel: RuntimeMetrics::default(),
        }));
        state.start_dispatcher();
        Ok(state)
    }
    fn start_dispatcher(&self) {
        let state = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(2));
            loop {
                tick.tick().await;
                if state.0.draining.load(Ordering::Relaxed) && state.0.pending.lock().is_empty() {
                    break;
                }
                let Some(req) = state.0.scheduler.pop_next() else {
                    continue;
                };
                let Some(pending) = state.0.pending.lock().remove(&req.id) else {
                    continue;
                };
                let Some(pool) = state.0.pools.get(&req.pool) else {
                    continue;
                };
                let config = state.0.config.read().await.clone();
                if !state.try_enter(&req.identity, &config) {
                    if pending.queued_at.elapsed() >= pending.request.deadline {
                        state.0.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    let _ = state.0.scheduler.enqueue(
                        pending.request.clone(),
                        pending.body_bytes,
                        pending.class_weight,
                        pending.tenant_weight,
                        pending.app_weight,
                    );
                    state.0.pending.lock().insert(req.id, pending);
                    continue;
                }
                let charged = pool.capacity.charged_units(req.estimate.normalized_units);
                let reservation = state
                    .reserve_distributed(&req, &config, pool.limit, charged)
                    .await;
                if let Ok(Some(leases)) = reservation {
                    pool.capacity.track_distributed(leases.capacity.id, charged);
                    if let Err(leases) = pending.sender.send(leases) {
                        state.release_distributed(&leases).await;
                        pool.capacity.release(
                            leases.capacity.id,
                            req.estimate.normalized_units,
                            None,
                            false,
                        );
                        state.leave(&req.identity);
                    }
                } else {
                    state.leave(&req.identity);
                    let elapsed = pending.queued_at.elapsed();
                    if elapsed >= pending.request.deadline {
                        state.0.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    let _ = state.0.scheduler.enqueue(
                        pending.request.clone(),
                        pending.body_bytes,
                        pending.class_weight,
                        pending.tenant_weight,
                        pending.app_weight,
                    );
                    state.0.pending.lock().insert(req.id, pending);
                }
            }
        });
    }
    pub fn data_router(&self) -> Router {
        Router::new()
            .fallback(any(proxy_handler))
            .with_state(self.clone())
    }
    pub fn admin_router(&self) -> Router {
        Router::new()
            .route("/health/live", get(live))
            .route("/health/ready", get(ready))
            .route("/metrics", get(metrics))
            .route("/api/v1/status", get(status))
            .route("/api/v1/capacity", get(capacity))
            .route("/api/v1/queues", get(queues))
            .route("/api/v1/decisions", get(decisions))
            .route("/ui", get(dashboard))
            .route("/ui/app.js", get(dashboard_js))
            .route("/ui/style.css", get(dashboard_css))
            .layer(middleware::from_fn_with_state(self.clone(), admin_security))
            .with_state(self.clone())
    }
    pub fn begin_drain(&self) {
        self.0.draining.store(true, Ordering::SeqCst)
    }
    /// Poll a local configuration file and atomically apply safe policy changes after validation.
    /// Listener, coordinator, pool, and OIDC trust-root changes require a restart.
    pub fn start_config_reload(&self, path: PathBuf, interval: Duration) {
        let state = self.clone();
        tokio::spawn(async move {
            let mut last = tokio::fs::read(&path).await.ok();
            let mut tick = tokio::time::interval(interval.max(Duration::from_millis(250)));
            loop {
                tick.tick().await;
                if state.0.draining.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(raw) = tokio::fs::read(&path).await else {
                    continue;
                };
                if last.as_deref() == Some(raw.as_slice()) {
                    continue;
                }
                let result = match Config::from_path(&path) {
                    Ok(new) => {
                        let current = state.0.config.read().await.clone();
                        ensure_reload_compatible(&current, &new)
                            .map_err(inferqos_config::ConfigError::Semantic)
                            .and_then(|()| {
                                state
                                    .0
                                    .identity
                                    .replace_config(new.clone())
                                    .map_err(|error| {
                                        inferqos_config::ConfigError::Semantic(error.to_string())
                                    })
                            })
                            .map(|()| new)
                    }
                    Err(error) => Err(error),
                };
                match result {
                    Ok(new) => {
                        *state.0.config.write().await = new;
                        last = Some(raw);
                        tracing::info!(path=%path.display(), "validated configuration reloaded");
                    }
                    Err(error) => {
                        tracing::error!(%error, path=%path.display(), "configuration reload rejected; retaining known-good configuration")
                    }
                }
            }
        });
    }
    pub fn active(&self) -> u64 {
        self.0.metrics.active.load(Ordering::Relaxed)
    }
    fn try_enter(&self, identity: &IdentityContext, config: &Config) -> bool {
        let tenant_limit = config
            .policies
            .tenants
            .get(&identity.tenant)
            .map_or(1, |policy| policy.max_concurrency);
        let application_limit = config
            .policies
            .applications
            .get(&identity.application)
            .map_or(1, |policy| policy.max_concurrency);
        let mut active = self.0.concurrency.lock();
        if active.tenants.get(&identity.tenant).copied().unwrap_or(0) >= tenant_limit
            || active
                .applications
                .get(&identity.application)
                .copied()
                .unwrap_or(0)
                >= application_limit
        {
            return false;
        }
        *active.tenants.entry(identity.tenant.clone()).or_default() += 1;
        *active
            .applications
            .entry(identity.application.clone())
            .or_default() += 1;
        true
    }
    fn force_enter(&self, identity: &IdentityContext) {
        let mut active = self.0.concurrency.lock();
        *active.tenants.entry(identity.tenant.clone()).or_default() += 1;
        *active
            .applications
            .entry(identity.application.clone())
            .or_default() += 1;
    }
    fn leave(&self, identity: &IdentityContext) {
        let mut active = self.0.concurrency.lock();
        decrement(&mut active.tenants, &identity.tenant);
        decrement(&mut active.applications, &identity.application);
    }
    fn tenant_capacity_limit(&self, config: &Config, tenant: &str, pool_limit: f64) -> f64 {
        let policy = config.policies.tenants.get(tenant);
        let own_guarantee = policy.map_or(0.0, |value| value.guaranteed_share);
        let configured_max = policy.map_or(1.0, |value| value.max_share);
        let pending = self.0.pending.lock();
        let competing: std::collections::HashSet<&str> = pending
            .values()
            .map(|value| value.request.identity.tenant.as_str())
            .filter(|value| *value != tenant)
            .collect();
        if competing.is_empty() {
            return pool_limit * configured_max;
        }
        let competing_guarantees: f64 = competing
            .into_iter()
            .map(|name| {
                config
                    .policies
                    .tenants
                    .get(name)
                    .map_or(0.0, |value| value.guaranteed_share)
            })
            .sum();
        let reclaimable_share = (1.0 - competing_guarantees.min(1.0)).max(own_guarantee);
        pool_limit * configured_max.min(reclaimable_share)
    }
    async fn reserve_distributed(
        &self,
        request: &AdmissionRequest,
        config: &Config,
        pool_limit: f64,
        charged: f64,
    ) -> Result<Option<AdmissionLeases>, inferqos_coordinator::CoordinatorError> {
        let tenant_concurrency = config
            .policies
            .tenants
            .get(&request.identity.tenant)
            .map_or(1, |policy| policy.max_concurrency) as f64;
        let application_concurrency = config
            .policies
            .applications
            .get(&request.identity.application)
            .map_or(1, |policy| policy.max_concurrency)
            as f64;
        let tenant_key = format!(
            "__concurrency__:{}:tenant:{}",
            request.pool, request.identity.tenant
        );
        let application_key = format!(
            "__concurrency__:{}:application:{}",
            request.pool, request.identity.application
        );
        let Some(tenant_slot) = self
            .0
            .coordinator
            .reserve(&tenant_key, tenant_concurrency, 1.0, self.0.lease_ttl)
            .await?
        else {
            return Ok(None);
        };
        let application_slot = match self
            .0
            .coordinator
            .reserve(
                &application_key,
                application_concurrency,
                1.0,
                self.0.lease_ttl,
            )
            .await?
        {
            Some(lease) => lease,
            None => {
                self.0.coordinator.release(&tenant_slot).await?;
                return Ok(None);
            }
        };
        let tenant_limit = self.tenant_capacity_limit(config, &request.identity.tenant, pool_limit);
        let capacity = match self
            .0
            .coordinator
            .reserve_scoped(
                &request.pool,
                &request.identity.tenant,
                pool_limit,
                tenant_limit,
                charged,
                self.0.lease_ttl,
            )
            .await?
        {
            Some(lease) => lease,
            None => {
                self.0.coordinator.release(&application_slot).await?;
                self.0.coordinator.release(&tenant_slot).await?;
                return Ok(None);
            }
        };
        Ok(Some(AdmissionLeases {
            capacity,
            tenant_slot,
            application_slot,
        }))
    }
    async fn release_distributed(&self, leases: &AdmissionLeases) {
        for lease in [
            &leases.capacity,
            &leases.application_slot,
            &leases.tenant_slot,
        ] {
            if let Err(error) = self.0.coordinator.release(lease).await {
                tracing::error!(%error, lease_id=%lease.id, "failed to release distributed lease; expiry will recover it");
            }
        }
    }
    fn record(&self, r: DecisionRecord, max: usize) {
        if max == 0 {
            return;
        }
        let mut d = self.0.decisions.lock();
        while d.len() >= max {
            d.pop_front();
        }
        d.push_back(r)
    }
}

fn decrement(values: &mut HashMap<String, usize>, key: &str) {
    if let Some(value) = values.get_mut(key) {
        *value = value.saturating_sub(1);
        if *value == 0 {
            values.remove(key);
        }
    }
}

fn ensure_reload_compatible(current: &Config, new: &Config) -> Result<(), String> {
    let current = serde_json::to_value((
        &current.server.listen,
        &current.server.tls,
        &current.admin.listen,
        &current.coordinator,
        &current.pools,
        &current.identity.oidc,
    ))
    .map_err(|error| error.to_string())?;
    let new = serde_json::to_value((
        &new.server.listen,
        &new.server.tls,
        &new.admin.listen,
        &new.coordinator,
        &new.pools,
        &new.identity.oidc,
    ))
    .map_err(|error| error.to_string())?;
    let unchanged = current == new;
    if !unchanged {
        return Err("listener, coordinator, capacity pool, and OIDC issuer changes require a rolling restart".into());
    }
    Ok(())
}

async fn proxy_handler(State(state): State<AppState>, request: Request) -> Response {
    let decision_started = Instant::now();
    if state.0.draining.load(Ordering::Relaxed) {
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "draining",
            "InferQoS is draining and is not accepting new work",
            Some(1),
        );
    }
    state.0.metrics.requests.fetch_add(1, Ordering::Relaxed);
    let config = state.0.config.read().await.clone();
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|value| value.0);
    let mtls_fingerprints = request
        .extensions()
        .get::<VerifiedClientCertificate>()
        .map_or_else(Vec::new, |value| value.sha256_fingerprints.clone());
    let (max_body, method, uri, headers) = (
        config.server.max_body_bytes,
        request.method().clone(),
        request.uri().clone(),
        request.headers().clone(),
    );
    let body = match axum::body::to_bytes(request.into_body(), max_body).await {
        Ok(b) => b,
        Err(_) => {
            return problem(
                StatusCode::PAYLOAD_TOO_LARGE,
                "body_too_large",
                &format!("request body exceeds configured limit of {max_body} bytes"),
                None,
            );
        }
    };
    let request_id = headers
        .get(HEADER_REQUEST_ID)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok())
        .unwrap_or_else(Uuid::new_v4);
    let identity = match state
        .0
        .identity
        .resolve(&headers, peer.map(|value| value.ip()), &mtls_fingerprints)
        .await
    {
        Ok(identity) => identity,
        Err(error) => {
            return problem(
                StatusCode::UNAUTHORIZED,
                "authentication_failed",
                &error.to_string(),
                None,
            );
        }
    };
    let requested = headers
        .get(HEADER_CLASS)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(ServiceClass::Standard);
    let (effective, app_policy) = effective_class(&config, &identity, requested);
    let pool_name = app_policy
        .and_then(|p| p.permitted_pools.iter().next().cloned())
        .or_else(|| config.pools.keys().next().cloned())
        .expect("validated pool exists");
    let Some(pool) = state.0.pools.get(&pool_name) else {
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "pool_unavailable",
            "configured pool is unavailable",
            Some(1),
        );
    };
    let estimation_request = ProxyRequest {
        method: method.clone(),
        uri: uri.clone(),
        headers: headers.clone(),
        body: body.clone(),
    };
    let estimate = match pool.provider.estimate(&estimation_request).await {
        Ok(e) => e,
        Err(e) => {
            return problem(
                StatusCode::BAD_REQUEST,
                "estimate_failed",
                &e.to_string(),
                None,
            );
        }
    };
    state
        .0
        .otel
        .request(&effective.to_string(), estimate.normalized_units.0);
    let stored_body = match StoredBody::store(
        body,
        config.server.spool_threshold_bytes,
        &config.server.spool_directory,
    )
    .await
    {
        Ok(body) => body,
        Err(error) => {
            return problem(
                StatusCode::INSUFFICIENT_STORAGE,
                "spool_failed",
                &format!("cannot securely spool bounded request body: {error}"),
                None,
            );
        }
    };
    let default_class = &config.service_classes[&effective.to_string()];
    let deadline = headers
        .get(HEADER_DEADLINE_MS)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(default_class.default_deadline)
        .min(default_class.max_queue);
    let queueable = headers
        .get(HEADER_QUEUEABLE)
        .and_then(|v| v.to_str().ok())
        .map_or(!matches!(effective, ServiceClass::Realtime), |v| {
            v.eq_ignore_ascii_case("true")
        });
    let admission = AdmissionRequest {
        id: request_id,
        identity: identity.clone(),
        requested_class: requested,
        effective_class: effective,
        pool: pool_name.clone(),
        estimate: estimate.clone(),
        deadline,
        queueable,
    };
    let charged = pool.capacity.charged_units(estimate.normalized_units);
    let mut concurrency_entered = state.try_enter(&identity, &config);
    let immediate = if concurrency_entered {
        match state
            .reserve_distributed(&admission, &config, pool.limit, charged)
            .await
        {
            Ok(leases) => {
                if leases.is_none() {
                    state.leave(&identity);
                    concurrency_entered = false;
                }
                leases
            }
            Err(error) => {
                state.leave(&identity);
                state.0.otel.coordinator_failure();
                return problem(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "coordinator_unavailable",
                    &format!("capacity coordinator failed closed: {error}"),
                    Some(1),
                );
            }
        }
    } else {
        None
    };
    if let Some(leases) = &immediate {
        pool.capacity.track_distributed(leases.capacity.id, charged);
    }
    let (mut reservation, mut outcome, mut queue_age) = (immediate, "admitted", 0u64);
    if reservation.is_none() {
        if config.mode == Mode::Shadow {
            state
                .0
                .metrics
                .shadow_would_queue
                .fetch_add(1, Ordering::Relaxed);
            outcome = "shadow_would_queue";
            state.force_enter(&identity);
            concurrency_entered = true;
        } else if !queueable {
            return reject_and_record(
                &state,
                &config,
                &admission,
                "capacity unavailable and request is not queueable",
            );
        } else {
            let (tx, rx) = oneshot::channel();
            let tenant_weight = config
                .policies
                .tenants
                .get(&identity.tenant)
                .map_or(1, |t| t.weight);
            let app_weight = app_policy.map_or(1, |a| a.weight);
            if let Err(e) = state.0.scheduler.enqueue(
                admission.clone(),
                stored_body.len(),
                default_class.weight,
                tenant_weight,
                app_weight,
            ) {
                return reject_and_record(&state, &config, &admission, &e.to_string());
            }
            state.0.metrics.queued.fetch_add(1, Ordering::Relaxed);
            state.0.pending.lock().insert(
                request_id,
                Pending {
                    sender: tx,
                    request: admission.clone(),
                    body_bytes: stored_body.len(),
                    class_weight: default_class.weight,
                    tenant_weight,
                    app_weight,
                    queued_at: Instant::now(),
                },
            );
            let wait_start = Instant::now();
            match tokio::time::timeout(deadline, rx).await {
                Ok(Ok(id)) => {
                    reservation = Some(id);
                    concurrency_entered = true;
                    outcome = "queued_then_admitted";
                    queue_age = wait_start.elapsed().as_millis() as u64
                }
                _ => {
                    state.0.scheduler.cancel(request_id);
                    state.0.pending.lock().remove(&request_id);
                    return reject_and_record(
                        &state,
                        &config,
                        &admission,
                        "queue deadline expired",
                    );
                }
            }
        }
    }
    state.0.otel.admission(
        decision_started.elapsed().as_secs_f64(),
        &effective.to_string(),
    );
    if queue_age > 0 {
        state
            .0
            .otel
            .queue(queue_age as f64 / 1000.0, &effective.to_string());
    }
    state.0.metrics.admitted.fetch_add(1, Ordering::Relaxed);
    state.0.metrics.active.fetch_add(1, Ordering::Relaxed);
    state.0.otel.active(1, &effective.to_string());
    let admission_guard = ActiveAdmissionGuard {
        state: state.clone(),
        capacity: pool.capacity.clone(),
        leases: reservation.take(),
        identity: identity.clone(),
        estimate: estimate.clone(),
        effective_class: effective.to_string(),
        concurrency_entered,
        counted_active: true,
    };
    state.record(
        DecisionRecord {
            request_id,
            effective_class: effective.to_string(),
            tenant: identity.tenant.clone(),
            application: identity.application.clone(),
            pool: pool_name.clone(),
            estimated_work_units: estimate.normalized_units.0,
            outcome: outcome.into(),
            queue_age_ms: queue_age,
        },
        config.limits.decision_history,
    );
    let dispatch_body = match stored_body.load().await {
        Ok(body) => body,
        Err(error) => {
            return problem(
                StatusCode::INTERNAL_SERVER_ERROR,
                "spool_read_failed",
                &format!("queued request body could not be read safely: {error}"),
                None,
            );
        }
    };
    let proxy_request = ProxyRequest {
        method,
        uri,
        headers,
        body: dispatch_body,
    };
    let result = pool.provider.dispatch(proxy_request).await;
    let response = match result {
        Ok(r) => r,
        Err(e) => {
            return problem(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                &e.to_string(),
                None,
            );
        }
    };
    let throttled = response.status == StatusCode::TOO_MANY_REQUESTS;
    if throttled {
        state
            .0
            .metrics
            .provider_throttles
            .fetch_add(1, Ordering::Relaxed);
        state.0.otel.throttle(pool.provider.name());
    }
    let status_code = response.status;
    let mut response_headers = response.headers;
    let mut upstream_rx = response.body;
    let usage = response.usage;
    let (output_tx, output_rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, CoreError>>(16);
    tokio::spawn(async move {
        let mut renew = tokio::time::interval(
            (admission_guard.state.0.lease_ttl / 3).max(Duration::from_secs(1)),
        );
        loop {
            tokio::select! {
                chunk = upstream_rx.recv() => match chunk {
                    Some(chunk) => if output_tx.send(chunk).await.is_err() { break; },
                    None => break,
                },
                _ = renew.tick(), if admission_guard.leases.is_some() => {
                    if let Some(leases) = &admission_guard.leases {
                        for lease in [&leases.capacity, &leases.tenant_slot, &leases.application_slot] {
                            if let Err(error) = admission_guard.state.0.coordinator.renew(lease, admission_guard.state.0.lease_ttl).await {
                                tracing::error!(%error, lease_id=%lease.id, "failed to renew active distributed lease");
                            }
                        }
                    }
                }
            }
        }
        let actual = usage.borrow().clone();
        admission_guard.complete(actual, throttled).await;
    });
    let body_stream = stream::unfold(output_rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|item| (item.map_err(std::io::Error::other), rx))
    });
    let mut out = Response::new(Body::from_stream(body_stream));
    *out.status_mut() = status_code;
    for (name, value) in response_headers.drain() {
        if let Some(name) = name
            && response_header_safe(name.as_str())
        {
            out.headers_mut().append(name, value);
        }
    }
    out.headers_mut().insert(
        HeaderName::from_static(HEADER_REQUEST_ID),
        HeaderValue::from_str(&request_id.to_string()).expect("UUID is a header value"),
    );
    out
}

fn effective_class<'a>(
    config: &'a Config,
    identity: &IdentityContext,
    requested: ServiceClass,
) -> (ServiceClass, Option<&'a ApplicationPolicy>) {
    let app = config.policies.applications.get(&identity.application);
    let allowed = app.is_some_and(|p| {
        p.tenant == identity.tenant && p.allowed_classes.contains(&requested.to_string())
    });
    if identity.trusted && allowed {
        (requested, app)
    } else {
        (ServiceClass::Standard, app)
    }
}
fn reject_and_record(
    state: &AppState,
    config: &Config,
    request: &AdmissionRequest,
    reason: &str,
) -> Response {
    state.0.metrics.rejected.fetch_add(1, Ordering::Relaxed);
    state.record(
        DecisionRecord {
            request_id: request.id,
            effective_class: request.effective_class.to_string(),
            tenant: request.identity.tenant.clone(),
            application: request.identity.application.clone(),
            pool: request.pool.clone(),
            estimated_work_units: request.estimate.normalized_units.0,
            outcome: format!("rejected: {reason}"),
            queue_age_ms: 0,
        },
        config.limits.decision_history,
    );
    problem(
        StatusCode::TOO_MANY_REQUESTS,
        "qos_rejected",
        reason,
        Some(request.deadline.as_secs().max(1)),
    )
}
fn response_header_safe(name: &str) -> bool {
    !matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "set-cookie"
    )
}
fn problem(status: StatusCode, code: &str, detail: &str, retry: Option<u64>) -> Response {
    let mut r = (
        status,
        axum::Json(serde_json::json!({"error":{"code":code,"message":detail}})),
    )
        .into_response();
    if let Some(seconds) = retry {
        r.headers_mut().insert(
            "retry-after",
            HeaderValue::from_str(&seconds.to_string()).expect("integer header"),
        );
    }
    r
}
async fn live() -> impl IntoResponse {
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({"status":"live"})),
    )
}
async fn ready(State(s): State<AppState>) -> Response {
    if s.0.draining.load(Ordering::Relaxed) {
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "draining",
            "instance is draining",
            None,
        );
    }

    if s.0.coordinator.healthy().await.is_err() {
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "coordinator_unavailable",
            "the configured capacity coordinator is unavailable",
            Some(1),
        );
    }

    let mut usable_pools = 0usize;
    for pool in s.0.pools.values() {
        if matches!(pool.provider.health().await, Ok(health) if health.healthy) {
            usable_pools += 1;
        }
    }
    if usable_pools == 0 {
        return problem(
            StatusCode::SERVICE_UNAVAILABLE,
            "no_usable_pool",
            "no configured inference capacity pool is currently usable",
            Some(1),
        );
    }

    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "status":"ready",
            "pools":s.0.pools.len(),
            "usable_pools":usable_pools
        })),
    )
        .into_response()
}
async fn status(State(s): State<AppState>) -> impl IntoResponse {
    axum::Json(
        serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"draining":s.0.draining.load(Ordering::Relaxed),"active":s.active(),"queue":s.0.scheduler.snapshot()}),
    )
}
async fn capacity(State(s): State<AppState>) -> impl IntoResponse {
    let map: BTreeMap<_, _> =
        s.0.pools
            .iter()
            .map(|(k, v)| (k.clone(), v.capacity.status()))
            .collect();
    axum::Json(map)
}
async fn queues(State(s): State<AppState>) -> impl IntoResponse {
    axum::Json(s.0.scheduler.snapshot())
}
async fn decisions(State(s): State<AppState>) -> impl IntoResponse {
    axum::Json(s.0.decisions.lock().iter().cloned().collect::<Vec<_>>())
}
async fn metrics(State(s): State<AppState>) -> String {
    let mut output = format!(
        "# TYPE inferqos_requests_total counter\ninferqos_requests_total {}\n# TYPE inferqos_admitted_total counter\ninferqos_admitted_total {}\n# TYPE inferqos_queued_total counter\ninferqos_queued_total {}\n# TYPE inferqos_rejected_total counter\ninferqos_rejected_total {}\n# TYPE inferqos_shadow_would_queue_total counter\ninferqos_shadow_would_queue_total {}\n# TYPE inferqos_provider_throttles_total counter\ninferqos_provider_throttles_total {}\n# TYPE inferqos_active_requests gauge\ninferqos_active_requests {}\n# TYPE inferqos_queue_depth gauge\ninferqos_queue_depth {}\n",
        s.0.metrics.requests.load(Ordering::Relaxed),
        s.0.metrics.admitted.load(Ordering::Relaxed),
        s.0.metrics.queued.load(Ordering::Relaxed),
        s.0.metrics.rejected.load(Ordering::Relaxed),
        s.0.metrics.shadow_would_queue.load(Ordering::Relaxed),
        s.0.metrics.provider_throttles.load(Ordering::Relaxed),
        s.0.metrics.active.load(Ordering::Relaxed),
        s.0.scheduler.snapshot().depth
    );
    output.push_str("# TYPE inferqos_capacity_configured_units gauge\n# TYPE inferqos_capacity_reserved_units gauge\n# TYPE inferqos_capacity_safety_factor gauge\n# TYPE inferqos_capacity_confidence gauge\n");
    for (name, pool) in &s.0.pools {
        let name = prometheus_escape(name);
        let status = pool.capacity.status();
        output.push_str(&format!(
            "inferqos_capacity_configured_units{{pool=\"{name}\"}} {}\ninferqos_capacity_reserved_units{{pool=\"{name}\"}} {}\ninferqos_capacity_safety_factor{{pool=\"{name}\"}} {}\ninferqos_capacity_confidence{{pool=\"{name}\"}} {}\n",
            status.configured_units, status.reserved_units, status.safety_factor, status.confidence,
        ));
    }
    output
}
fn prometheus_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
async fn dashboard() -> Html<&'static str> {
    Html(DASHBOARD)
}
async fn dashboard_js() -> impl IntoResponse {
    (
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("cache-control", "no-store"),
        ],
        DASHBOARD_JS,
    )
}
async fn dashboard_css() -> impl IntoResponse {
    (
        [
            ("content-type", "text/css; charset=utf-8"),
            ("cache-control", "public, max-age=3600"),
        ],
        DASHBOARD_CSS,
    )
}
async fn admin_security(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let health = matches!(path, "/health/live" | "/health/ready");
    let config = state.0.config.read().await;
    if !health && let Some(environment) = &config.admin.bearer_token_env {
        let expected = match std::env::var(environment) {
            Ok(value) => value,
            Err(_) => {
                return problem(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "admin_auth_unavailable",
                    "configured admin bearer token environment variable is not set",
                    None,
                );
            }
        };
        let supplied = request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if !supplied.is_some_and(|value| expected.as_bytes().ct_eq(value.as_bytes()).into()) {
            return problem(
                StatusCode::UNAUTHORIZED,
                "admin_auth_required",
                "a valid admin bearer token is required",
                None,
            );
        }
    }
    drop(config);
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self'; frame-ancestors 'none'",
        ),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
        .headers_mut()
        .insert("x-frame-options", HeaderValue::from_static("DENY"));
    response
        .headers_mut()
        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response
}
const DASHBOARD: &str = r#"<!doctype html><html><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>InferQoS</title><link rel=stylesheet href=/ui/style.css></head><body><main><h1>InferQoS</h1><p>Finite inference capacity, scheduled fairly. No prompt data is shown or stored.</p><div id=grid class=grid></div></main><script src=/ui/app.js defer></script></body></html>"#;
const DASHBOARD_CSS: &str = "body{font:15px system-ui;background:#0b1020;color:#e5e7eb;margin:0}main{max-width:1040px;margin:56px auto;padding:24px}h1{font-size:42px}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(210px,1fr));gap:12px}.card{background:#151c31;border:1px solid #26304d;border-radius:12px;padding:18px}.value{font-size:28px;color:#7dd3fc}small{color:#94a3b8}";
const DASHBOARD_JS: &str = r#"async function refresh(){const [status,capacity,queue]=await Promise.all(['/api/v1/status','/api/v1/capacity','/api/v1/queues'].map(path=>fetch(path).then(response=>response.json())));const cards=[['Active',status.active],['Queue depth',queue.depth],['Pools',Object.keys(capacity).length],['Capacity reserved',Object.values(capacity).reduce((sum,pool)=>sum+pool.reserved_units,0).toFixed(1)]];const grid=document.getElementById('grid');grid.replaceChildren();for(const [label,value] of cards){const card=document.createElement('div');card.className='card';const caption=document.createElement('small');caption.textContent=label;const amount=document.createElement('div');amount.className='value';amount.textContent=String(value);card.append(caption,amount);grid.append(card)}}refresh();setInterval(refresh,2000);"#;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_set_cookie() {
        assert!(!response_header_safe("set-cookie"));
        assert!(response_header_safe("content-type"));
    }
}
