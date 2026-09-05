# Durable queue boundary

`DurableQueue` is optional and only for workflow/background/batch jobs. Realtime and interactive
admission never acquire durable-broker latency or semantics. All adapters enforce a bounded encoded
job size and provide at-least-once delivery around a stable UUID; consumers must make downstream
effects idempotent by that ID.

The built-in adapters are:

- `InMemoryDurableQueue`, for deterministic development, with explicit in-flight tracking.
- `NatsJetStreamQueue`, using a work-queue stream, durable pull consumer, publish acknowledgements,
  explicit double acknowledgement, a stable NATS message ID, and a ten-delivery ceiling. Supply
  NATS TLS, NKey, JWT, or credentials through `connect_with_options`.
- `AzureServiceBusQueue`, using the HTTPS REST peek-lock and settlement protocol. Authentication is
  a short-lived bearer-token provider; the `azure-auth` feature supplies managed-identity and
  workload-identity implementations. Settlement redirects are disabled and a returned settlement
  URL must remain on the configured namespace host.

The Service Bus adapter does not accept connection strings or embedded SAS keys. The environment
token provider exists for isolated contract tests only. Lock renewal and dead-letter policy remain
broker configuration concerns; an expired/unsettled delivery is intentionally eligible for
redelivery.
