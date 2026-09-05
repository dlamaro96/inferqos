//! Optional durable queues for workflow and batch traffic, never realtime admission.
//!
//! Network adapters provide at-least-once delivery. Downstream effects must use the stable job ID
//! for idempotency. Payloads are bounded before publication and after receipt.
#![forbid(unsafe_code)]

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, collections::VecDeque, sync::Arc, time::Duration};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

const DEFAULT_MAX_JOB_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableJob {
    pub id: Uuid,
    pub metadata: Bytes,
    pub payload: Bytes,
}

#[derive(Serialize, Deserialize)]
struct WireJob {
    id: Uuid,
    metadata: Vec<u8>,
    payload: Vec<u8>,
}

impl From<&DurableJob> for WireJob {
    fn from(job: &DurableJob) -> Self {
        Self {
            id: job.id,
            metadata: job.metadata.to_vec(),
            payload: job.payload.to_vec(),
        }
    }
}

impl From<WireJob> for DurableJob {
    fn from(job: WireJob) -> Self {
        Self {
            id: job.id,
            metadata: job.metadata.into(),
            payload: job.payload.into(),
        }
    }
}

#[async_trait]
pub trait DurableQueue: Send + Sync {
    async fn publish(&self, job: DurableJob) -> Result<(), QueueError>;
    async fn receive(&self) -> Result<Option<DurableJob>, QueueError>;
    async fn acknowledge(&self, id: Uuid) -> Result<(), QueueError>;
}

#[derive(Default)]
pub struct InMemoryDurableQueue {
    jobs: Mutex<VecDeque<DurableJob>>,
    in_flight: Mutex<HashMap<Uuid, DurableJob>>,
}

#[async_trait]
impl DurableQueue for InMemoryDurableQueue {
    async fn publish(&self, job: DurableJob) -> Result<(), QueueError> {
        ensure_size(&job, DEFAULT_MAX_JOB_BYTES)?;
        self.jobs.lock().push_back(job);
        Ok(())
    }

    async fn receive(&self) -> Result<Option<DurableJob>, QueueError> {
        let job = self.jobs.lock().pop_front();
        if let Some(job) = &job {
            self.in_flight.lock().insert(job.id, job.clone());
        }
        Ok(job)
    }

    async fn acknowledge(&self, id: Uuid) -> Result<(), QueueError> {
        self.in_flight
            .lock()
            .remove(&id)
            .ok_or(QueueError::UnknownDelivery(id))?;
        Ok(())
    }
}

/// NATS JetStream pull-consumer adapter with acknowledged publishing and explicit settlement.
pub struct NatsJetStreamQueue {
    context: async_nats::jetstream::Context,
    consumer: async_nats::jetstream::consumer::PullConsumer,
    subject: String,
    acknowledgements: tokio::sync::Mutex<HashMap<Uuid, async_nats::jetstream::message::Acker>>,
    max_job_bytes: usize,
}

impl NatsJetStreamQueue {
    pub async fn connect(
        server: &str,
        stream_name: &str,
        subject: &str,
        durable_consumer: &str,
        max_job_bytes: Option<usize>,
    ) -> Result<Self, QueueError> {
        Self::connect_with_options(
            async_nats::ConnectOptions::new(),
            server,
            stream_name,
            subject,
            durable_consumer,
            max_job_bytes,
        )
        .await
    }

    /// Connect with explicit NATS credentials, TLS roots, or NKey/JWT options.
    pub async fn connect_with_options(
        options: async_nats::ConnectOptions,
        server: &str,
        stream_name: &str,
        subject: &str,
        durable_consumer: &str,
        max_job_bytes: Option<usize>,
    ) -> Result<Self, QueueError> {
        let client = options.connect(server).await.map_err(unavailable)?;
        let context = async_nats::jetstream::new(client);
        let stream = context
            .get_or_create_stream(async_nats::jetstream::stream::Config {
                name: stream_name.to_owned(),
                subjects: vec![subject.to_owned()],
                retention: async_nats::jetstream::stream::RetentionPolicy::WorkQueue,
                ..Default::default()
            })
            .await
            .map_err(unavailable)?;
        let consumer = stream
            .get_or_create_consumer(
                durable_consumer,
                async_nats::jetstream::consumer::pull::Config {
                    durable_name: Some(durable_consumer.to_owned()),
                    filter_subject: subject.to_owned(),
                    ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
                    max_deliver: 10,
                    ..Default::default()
                },
            )
            .await
            .map_err(unavailable)?;
        Ok(Self {
            context,
            consumer,
            subject: subject.to_owned(),
            acknowledgements: tokio::sync::Mutex::new(HashMap::new()),
            max_job_bytes: max_job_bytes.unwrap_or(DEFAULT_MAX_JOB_BYTES),
        })
    }
}

#[async_trait]
impl DurableQueue for NatsJetStreamQueue {
    async fn publish(&self, job: DurableJob) -> Result<(), QueueError> {
        ensure_size(&job, self.max_job_bytes)?;
        let body = serde_json::to_vec(&WireJob::from(&job)).map_err(protocol)?;
        self.context
            .send_publish(
                self.subject.clone(),
                async_nats::jetstream::message::PublishMessage::build()
                    .message_id(job.id.to_string())
                    .payload(body.into()),
            )
            .await
            .map_err(unavailable)?
            .await
            .map_err(unavailable)?;
        Ok(())
    }

    async fn receive(&self) -> Result<Option<DurableJob>, QueueError> {
        let mut messages = self
            .consumer
            .fetch()
            .max_messages(1)
            .expires(Duration::from_secs(1))
            .messages()
            .await
            .map_err(unavailable)?;
        let Some(message) = messages.next().await else {
            return Ok(None);
        };
        let message = message.map_err(unavailable)?;
        if message.payload.len() > self.max_job_bytes.saturating_mul(2) {
            return Err(QueueError::TooLarge {
                actual: message.payload.len(),
                limit: self.max_job_bytes,
            });
        }
        let (message, acker) = message.split();
        let wire: WireJob = serde_json::from_slice(&message.payload).map_err(protocol)?;
        let job: DurableJob = wire.into();
        ensure_size(&job, self.max_job_bytes)?;
        self.acknowledgements.lock().await.insert(job.id, acker);
        Ok(Some(job))
    }

    async fn acknowledge(&self, id: Uuid) -> Result<(), QueueError> {
        let acker = self
            .acknowledgements
            .lock()
            .await
            .remove(&id)
            .ok_or(QueueError::UnknownDelivery(id))?;
        acker.double_ack().await.map_err(unavailable)
    }
}

#[async_trait]
pub trait BearerTokenProvider: Send + Sync {
    async fn token(&self, scopes: &[&str]) -> Result<String, QueueError>;
}

pub struct EnvironmentBearerToken(String);
impl EnvironmentBearerToken {
    pub fn new(variable: impl Into<String>) -> Self {
        Self(variable.into())
    }
}
#[async_trait]
impl BearerTokenProvider for EnvironmentBearerToken {
    async fn token(&self, _scopes: &[&str]) -> Result<String, QueueError> {
        std::env::var(&self.0).map_err(|_| {
            QueueError::Authentication(format!("token environment variable {} is not set", self.0))
        })
    }
}

/// Azure workload/managed-identity credential for Service Bus. It never stores
/// a long-lived access key and refreshes short-lived tokens through the Azure SDK.
#[cfg(feature = "azure-auth")]
pub struct AzureIdentityBearerToken {
    credential: Arc<dyn azure_core::credentials::TokenCredential>,
}

#[cfg(feature = "azure-auth")]
impl AzureIdentityBearerToken {
    pub fn managed_identity() -> Result<Self, QueueError> {
        azure_identity::ManagedIdentityCredential::new(None)
            .map(|credential| Self { credential })
            .map_err(|error| QueueError::Authentication(error.to_string()))
    }

    pub fn workload_identity() -> Result<Self, QueueError> {
        azure_identity::WorkloadIdentityCredential::new(None)
            .map(|credential| Self { credential })
            .map_err(|error| QueueError::Authentication(error.to_string()))
    }
}

#[cfg(feature = "azure-auth")]
#[async_trait]
impl BearerTokenProvider for AzureIdentityBearerToken {
    async fn token(&self, scopes: &[&str]) -> Result<String, QueueError> {
        self.credential
            .get_token(scopes, None)
            .await
            .map(|token| token.token.secret().to_owned())
            .map_err(|error| QueueError::Authentication(error.to_string()))
    }
}

/// Azure Service Bus REST adapter using peek-lock settlement and short-lived OAuth credentials.
pub struct AzureServiceBusQueue {
    client: reqwest::Client,
    endpoint: Url,
    queue: String,
    credential: Arc<dyn BearerTokenProvider>,
    settlements: tokio::sync::Mutex<HashMap<Uuid, Url>>,
    max_job_bytes: usize,
}

impl AzureServiceBusQueue {
    pub fn new(
        namespace_endpoint: &str,
        queue: impl Into<String>,
        credential: Arc<dyn BearerTokenProvider>,
        max_job_bytes: Option<usize>,
    ) -> Result<Self, QueueError> {
        let endpoint = Url::parse(namespace_endpoint).map_err(|error| {
            QueueError::Protocol(format!("invalid Service Bus endpoint: {error}"))
        })?;
        if endpoint.scheme() != "https" {
            return Err(QueueError::Protocol(
                "Azure Service Bus endpoint must use HTTPS".into(),
            ));
        }
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(70))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(unavailable)?,
            endpoint,
            queue: queue.into(),
            credential,
            settlements: tokio::sync::Mutex::new(HashMap::new()),
            max_job_bytes: max_job_bytes.unwrap_or(DEFAULT_MAX_JOB_BYTES),
        })
    }

    fn messages_url(&self, suffix: &str) -> Result<Url, QueueError> {
        self.endpoint
            .join(&format!("{}/messages{suffix}", self.queue))
            .map_err(protocol)
    }

    async fn bearer(&self) -> Result<String, QueueError> {
        self.credential
            .token(&["https://servicebus.azure.net/.default"])
            .await
            .map(|token| format!("Bearer {token}"))
    }
}

#[async_trait]
impl DurableQueue for AzureServiceBusQueue {
    async fn publish(&self, job: DurableJob) -> Result<(), QueueError> {
        ensure_size(&job, self.max_job_bytes)?;
        let body = serde_json::to_vec(&WireJob::from(&job)).map_err(protocol)?;
        self.client
            .post(self.messages_url("")?)
            .header("authorization", self.bearer().await?)
            .header("content-type", "application/json")
            .header(
                "brokerproperties",
                serde_json::json!({"MessageId":job.id.to_string()}).to_string(),
            )
            .body(body)
            .send()
            .await
            .map_err(unavailable)?
            .error_for_status()
            .map_err(unavailable)?;
        Ok(())
    }

    async fn receive(&self) -> Result<Option<DurableJob>, QueueError> {
        let response = self
            .client
            .post(self.messages_url("/head?timeout=1")?)
            .header("authorization", self.bearer().await?)
            .send()
            .await
            .map_err(unavailable)?;
        if matches!(response.status().as_u16(), 204 | 404) {
            return Ok(None);
        }
        let response = response.error_for_status().map_err(unavailable)?;
        if response
            .content_length()
            .is_some_and(|size| size > self.max_job_bytes.saturating_mul(2) as u64)
        {
            return Err(QueueError::TooLarge {
                actual: response.content_length().unwrap_or(u64::MAX) as usize,
                limit: self.max_job_bytes,
            });
        }
        let settlement = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                QueueError::Protocol("peek-lock response has no Location header".into())
            })?;
        let settlement = self.endpoint.join(settlement).map_err(protocol)?;
        if settlement.host_str() != self.endpoint.host_str() {
            return Err(QueueError::Protocol(
                "Service Bus settlement URL changed host".into(),
            ));
        }
        let bytes = response.bytes().await.map_err(unavailable)?;
        if bytes.len() > self.max_job_bytes.saturating_mul(2) {
            return Err(QueueError::TooLarge {
                actual: bytes.len(),
                limit: self.max_job_bytes,
            });
        }
        let job: DurableJob = serde_json::from_slice::<WireJob>(&bytes)
            .map_err(protocol)?
            .into();
        ensure_size(&job, self.max_job_bytes)?;
        self.settlements.lock().await.insert(job.id, settlement);
        Ok(Some(job))
    }

    async fn acknowledge(&self, id: Uuid) -> Result<(), QueueError> {
        let settlement = self
            .settlements
            .lock()
            .await
            .remove(&id)
            .ok_or(QueueError::UnknownDelivery(id))?;
        self.client
            .delete(settlement)
            .header("authorization", self.bearer().await?)
            .send()
            .await
            .map_err(unavailable)?
            .error_for_status()
            .map_err(unavailable)?;
        Ok(())
    }
}

fn ensure_size(job: &DurableJob, limit: usize) -> Result<(), QueueError> {
    let actual = job.metadata.len().saturating_add(job.payload.len());
    if actual > limit {
        return Err(QueueError::TooLarge { actual, limit });
    }
    Ok(())
}
fn unavailable(error: impl std::fmt::Display) -> QueueError {
    QueueError::Unavailable(error.to_string())
}
fn protocol(error: impl std::fmt::Display) -> QueueError {
    QueueError::Protocol(error.to_string())
}

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("durable queue unavailable: {0}")]
    Unavailable(String),
    #[error("durable queue authentication failed: {0}")]
    Authentication(String),
    #[error("durable queue protocol error: {0}")]
    Protocol(String),
    #[error("durable job is {actual} bytes, exceeding the {limit}-byte limit")]
    TooLarge { actual: usize, limit: usize },
    #[error("delivery {0} is not awaiting acknowledgement")]
    UnknownDelivery(Uuid),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_roundtrip_requires_acknowledgement() {
        let queue = InMemoryDurableQueue::default();
        let id = Uuid::new_v4();
        queue
            .publish(DurableJob {
                id,
                metadata: Bytes::new(),
                payload: Bytes::from_static(b"x"),
            })
            .await
            .expect("publish");
        assert_eq!(queue.receive().await.expect("receive").expect("job").id, id);
        queue.acknowledge(id).await.expect("ack");
        assert!(matches!(
            queue.acknowledge(id).await,
            Err(QueueError::UnknownDelivery(_))
        ));
    }

    #[test]
    fn service_bus_requires_tls() {
        let result = AzureServiceBusQueue::new(
            "http://example.test/",
            "jobs",
            Arc::new(EnvironmentBearerToken::new("TOKEN")),
            None,
        );
        assert!(matches!(result, Err(QueueError::Protocol(_))));
    }
}
