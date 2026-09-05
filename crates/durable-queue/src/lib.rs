//! Optional queue contract for workflow/batch jobs; never used by realtime admission.
#![forbid(unsafe_code)]
use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::Mutex;
use std::collections::VecDeque;
use thiserror::Error;
use uuid::Uuid;
#[derive(Debug, Clone)]
pub struct DurableJob {
    pub id: Uuid,
    pub metadata: Bytes,
    pub payload: Bytes,
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
}
#[async_trait]
impl DurableQueue for InMemoryDurableQueue {
    async fn publish(&self, job: DurableJob) -> Result<(), QueueError> {
        self.jobs.lock().push_back(job);
        Ok(())
    }
    async fn receive(&self) -> Result<Option<DurableJob>, QueueError> {
        Ok(self.jobs.lock().pop_front())
    }
    async fn acknowledge(&self, _: Uuid) -> Result<(), QueueError> {
        Ok(())
    }
}
#[derive(Debug, Error)]
pub enum QueueError {
    #[error("durable queue unavailable: {0}")]
    Unavailable(String),
    #[error("durable queue protocol error: {0}")]
    Protocol(String),
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn memory_roundtrip() {
        let q = InMemoryDurableQueue::default();
        let id = Uuid::new_v4();
        q.publish(DurableJob {
            id,
            metadata: Bytes::new(),
            payload: Bytes::from_static(b"x"),
        })
        .await
        .unwrap();
        assert_eq!(q.receive().await.unwrap().unwrap().id, id);
        q.acknowledge(id).await.unwrap();
    }
}
