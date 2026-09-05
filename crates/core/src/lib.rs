//! Stable, provider-neutral domain types used by every InferQoS component.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use serde::{Deserialize, Serialize};
use std::{fmt, time::Duration};
use thiserror::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum ServiceClass {
    Realtime,
    Interactive,
    Standard,
    Workflow,
    Batch,
}

impl fmt::Display for ServiceClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Realtime => "realtime",
            Self::Interactive => "interactive",
            Self::Standard => "standard",
            Self::Workflow => "workflow",
            Self::Batch => "batch",
        })
    }
}

impl std::str::FromStr for ServiceClass {
    type Err = CoreError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "realtime" => Ok(Self::Realtime),
            "interactive" => Ok(Self::Interactive),
            "standard" => Ok(Self::Standard),
            "workflow" => Ok(Self::Workflow),
            "batch" => Ok(Self::Batch),
            _ => Err(CoreError::InvalidServiceClass(value.into())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WorkUnits(pub f64);

impl WorkUnits {
    pub fn new(value: f64) -> Result<Self, CoreError> {
        if value.is_finite() && value >= 0.0 {
            Ok(Self(value))
        } else {
            Err(CoreError::InvalidWork(value))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateSource {
    ExactTokenizer,
    CompatibleTokenizer,
    TrustedClient,
    ProviderMetadata,
    Approximation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkEstimate {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub provider_cost_coefficient: f64,
    pub normalized_units: WorkUnits,
    pub confidence: f32,
    pub source: EstimateSource,
}

impl WorkEstimate {
    pub fn conservative(
        input_tokens: u64,
        max_output_tokens: u64,
        coefficient: f64,
    ) -> Result<Self, CoreError> {
        let units = (input_tokens.saturating_add(max_output_tokens)) as f64 * coefficient;
        Ok(Self {
            input_tokens,
            output_tokens: max_output_tokens,
            cached_input_tokens: 0,
            provider_cost_coefficient: coefficient,
            normalized_units: WorkUnits::new(units)?,
            confidence: 0.45,
            source: EstimateSource::Approximation,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdentityContext {
    pub principal: String,
    pub tenant: String,
    pub application: String,
    pub trusted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionRequest {
    pub id: Uuid,
    pub identity: IdentityContext,
    pub requested_class: ServiceClass,
    pub effective_class: ServiceClass,
    pub pool: String,
    pub estimate: WorkEstimate,
    pub deadline: Duration,
    pub queueable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum AdmissionDecision {
    Admit {
        reservation_id: Uuid,
    },
    Queue {
        position: usize,
        retry_after_ms: u64,
    },
    Reject {
        reason: String,
        retry_after_ms: Option<u64>,
    },
    Shadow {
        would: Box<AdmissionDecision>,
    },
}

#[derive(Debug, Clone)]
pub struct ProxyRequest {
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap,
    pub body: Bytes,
}

pub struct ProviderResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: mpsc::Receiver<Result<Bytes, CoreError>>,
    /// Updated when end-of-response usage metadata is observed. `None` means unavailable.
    pub usage: tokio::sync::watch::Receiver<Option<WorkEstimate>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamHealth {
    pub healthy: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThrottleSignal {
    pub retry_after: Option<Duration>,
    pub status: u16,
}

#[async_trait]
pub trait WorkEstimator: Send + Sync {
    async fn estimate(&self, request: &ProxyRequest) -> Result<WorkEstimate, CoreError>;
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    async fn estimate(&self, request: &ProxyRequest) -> Result<WorkEstimate, CoreError>;
    async fn dispatch(&self, request: ProxyRequest) -> Result<ProviderResponse, CoreError>;
    async fn health(&self) -> Result<UpstreamHealth, CoreError>;
}

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unknown service class '{0}'")]
    InvalidServiceClass(String),
    #[error("invalid normalized work value {0}")]
    InvalidWork(f64),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("request cancelled")]
    Cancelled,
    #[error("deadline expired")]
    Deadline,
    #[error("capacity unavailable; retry after {0:?}")]
    Capacity(Option<Duration>),
}

pub const HEADER_CLASS: &str = "x-inferqos-class";
pub const HEADER_DEADLINE_MS: &str = "x-inferqos-deadline-ms";
pub const HEADER_TENANT: &str = "x-inferqos-tenant";
pub const HEADER_APPLICATION: &str = "x-inferqos-application";
pub const HEADER_QUEUEABLE: &str = "x-inferqos-queueable";
pub const HEADER_REQUEST_ID: &str = "x-inferqos-request-id";
