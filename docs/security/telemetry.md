# Telemetry and privacy

Default policy:

```text
No project analytics.
No anonymous analytics.
No outbound telemetry except explicitly configured provider and OTLP endpoints.
Prompt logging = off. Completion logging = off. Phone-home = off.
```

The data plane observes headers, endpoint path, body size, estimated/actual token metadata, timing,
status, and throttle signals. Request content is forwarded but not logged. Small queued bodies may
occupy bounded memory; larger bounded bodies may use secure auto-deleted local spool files according
to configuration. Decision history stores identity labels, class, pool, work estimate, outcome, and
wait time only and may be disabled with `decision_history: 0`.
