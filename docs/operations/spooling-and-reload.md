# Secure spooling and configuration reload

Request bodies are always bounded by `server.max_body_bytes`. Bodies larger than
`server.spool_threshold_bytes` are moved to an InferQoS-owned spool directory created with mode
0700. Each file is created atomically with an exclusive handle, written through that still-open
handle, reopened only for dispatch, and removed automatically when the queued request completes or
is dropped. Configure a dedicated encrypted ephemeral volume in production. The spool is not a
durable queue and must not be backed up.

InferQoS polls the configuration at `server.config_reload_interval` (minimum effective interval
250 ms). A candidate is environment-expanded, strictly parsed, and semantically validated before
an atomic policy swap. Invalid changes are logged and the known-good configuration stays live.
Service-class weights/deadlines, policy, identity mappings, proxy trust, queue limits, and mode can
reload. Listener addresses, TLS trust, coordinator, capacity pools, and OIDC issuer/JWKS source
require a rolling restart because they own live network or credential state.
