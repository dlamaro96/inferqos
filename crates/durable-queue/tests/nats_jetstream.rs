// SPDX-License-Identifier: Apache-2.0
use bytes::Bytes;
use inferqos_durable_queue::{DurableJob, DurableQueue, NatsJetStreamQueue};
use uuid::Uuid;

#[tokio::test]
async fn nats_publish_receive_and_double_ack() {
    let Ok(server) = std::env::var("INFERQOS_TEST_NATS_URL") else {
        eprintln!("skipping credential/service-backed NATS test; INFERQOS_TEST_NATS_URL is unset");
        return;
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let subject = format!("inferqos.jobs.{suffix}");
    let queue = NatsJetStreamQueue::connect(
        &server,
        &format!("INFERQOS_{suffix}"),
        &subject,
        &format!("worker-{suffix}"),
        Some(1024),
    )
    .await
    .expect("connect to NATS JetStream");
    let expected = DurableJob {
        id: Uuid::new_v4(),
        metadata: Bytes::from_static(b"{}"),
        payload: Bytes::from_static(b"work"),
    };
    queue
        .publish(expected.clone())
        .await
        .expect("publish acknowledged");
    let actual = queue
        .receive()
        .await
        .expect("receive")
        .expect("job available");
    assert_eq!(actual, expected);
    queue
        .acknowledge(actual.id)
        .await
        .expect("server confirmed acknowledgement");
}
