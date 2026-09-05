# Changelog

## 0.2.0 - 2026-09-05

- Rebuilt the embedded management dashboard around live capacity pressure, calibration confidence,
  class outcomes, runtime health, and bounded explainable decision metadata.
- Linked a separately maintained public website with responsive light/dark modes and an interactive
  deterministic `/demo/` scheduler comparison; its source and deployment remain outside the OSS
  product repository.
- Added isolated PlugLayer deployment assets, tests, and documentation without making PlugLayer a
  runtime dependency.
- Expanded the management status API with mode, uptime, and admission counters while preserving
  content-free telemetry and strict browser security headers.

## 0.1.0 - 2026-09-05

- Initial finite-capacity QoS proxy, scheduler, adaptive ledger, Valkey coordinator, provider
  adapters, shadow mode, replay simulator, operational API/dashboard, SDK helpers, and deployments.
- Added bounded NATS JetStream and Azure Service Bus durable queues.
- Added OIDC/JWKS, direct and proxied mTLS identity, native cloud workload authentication, OTLP,
  secure disk spooling, atomic configuration reload, and a versioned external-provider gRPC API.
- Added distributed share and concurrency enforcement, streaming token reconciliation, chaos/load/
  fuzz suites, multi-architecture release verification, and Sigstore-verified installers.
