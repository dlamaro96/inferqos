---
title: "Telemetry operations"
---

# Telemetry operations

Logs are structured JSON. Set `RUST_LOG` for filtering. Prometheus metrics remain available at the
private admin listener's `/metrics` route.

Setting `OTEL_EXPORTER_OTLP_ENDPOINT` enables OTLP/gRPC batch trace export and periodic metric
export. Standard OpenTelemetry environment variables such as OTLP headers and TLS settings are
handled by the exporter. Startup fails visibly when exporter configuration is invalid; temporary
export failure is reported by the OpenTelemetry SDK and never changes an admission decision.

Runtime instruments cover request and active counts, admission and queue duration, predicted and
actual normalized work, prediction error, provider throttles, and coordinator failure. Prometheus
also exposes pool reservations, safety factor, confidence, queue depth, admission/rejection, and
shadow decisions. Labels are bounded to configured service classes and pools. Prompts,
completions, raw user IDs, credentials, and bearer tokens are never attributes.
