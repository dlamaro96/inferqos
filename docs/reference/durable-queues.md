# Durable queue boundary

`DurableQueue` is optional and only for workflow/background/batch jobs. The in-memory adapter is for
development. NATS JetStream and Azure Service Bus production integrations belong outside the
interactive request path and must provide explicit publish, receive/lease, acknowledge, retry, and
dead-letter semantics. Realtime requests never acquire durable-broker latency or semantics.

