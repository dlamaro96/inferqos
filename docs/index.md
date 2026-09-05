# InferQoS documentation

InferQoS makes finite AI inference capacity a shared, schedulable resource. It performs admission,
fair scheduling, and capacity accounting; it is deliberately not a general AI gateway.

## Start here

- [Five-minute local demo](https://github.com/dlamaro96/inferqos#run-the-live-demo): no cloud account or paid API key required
- [Architecture and trust boundaries](architecture/overview.md)
- [Scheduling, deadlines, and fairness](concepts/scheduling.md)
- [Deployment guide](deployment/index.md)
- [PlugLayer deployment boundary](deployment/pluglayer.md)
- [Provider configuration](providers.md)

## Operate and extend

- [Identity: OIDC, mTLS, API keys, and trusted proxies](security/identity.md)
- [Privacy and telemetry policy](security/telemetry.md)
- [OTLP and Prometheus operations](operations/telemetry.md)
- [Embedded operations dashboard](operations/dashboard.md)
- [Secure request spooling and hot reload](operations/spooling-and-reload.md)
- [Durable queue adapters](reference/durable-queues.md)
- [External provider gRPC protocol](reference/external-provider-protocol.md)
- [Provider SDK](reference/provider-sdk.md)
- [QoS headers](reference/qos-protocol.md)

The default configuration records no prompts or completions and sends no project analytics.
