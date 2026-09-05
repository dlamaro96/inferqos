# Threat model

| Threat | Control |
|---|---|
| Spoofed QoS/tenant headers | Entitlement resolution from authenticated mapping; untrusted requests become `standard`. |
| Tenant starvation | Weighted work accounting, max share/concurrency policy, aging, bounded admission. |
| Queue/memory exhaustion | Global depth/byte/body limits and early 413/429 backpressure. |
| Admin exposure | Separate loopback listener by default; internal ingress guidance. |
| SSRF | No per-request target; HTTPS except loopback; optional exact host allowlist. |
| Credential/log leakage | Environment/file/workload identity; auth headers stripped; no content logging. |
| Malicious provider response | Hop-by-hop and `set-cookie` response stripping; bounded channels. |
| Coordinator poisoning/replay | Namespace isolation, TLS/ACL guidance, random lease IDs, idempotent release, expiry. |
| Crash during stream | lease TTL recovery in HA; conservative capacity correction. |
| Config tampering/injection | strict unknown-field rejection, semantic validation, environment-name validation. |

Deploy with least-privilege identities, egress policy, rustls TLS, non-root/read-only containers,
network policies, coordinator TLS/ACLs, and a private admin route. InferQoS cannot protect a host or
configuration source already controlled by an attacker.

