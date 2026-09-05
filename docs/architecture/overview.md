---
title: "Architecture"
---

# Architecture

![InferQoS request path, trust boundaries, providers, HA coordinator, and admin plane](../assets/architecture.svg)

```text
untrusted client → [trusted gateway] → data plane → provider
                                      │
                              identity / policy
                                      ↓
                           estimate → admission
                                      ↓
                           fair scheduler / ledger
                                      │
                         Valkey coordinator (HA only)

internal operator → admin plane → metrics / status / decisions
                                      ↓
                           configured OTel backend
```

Client identity and QoS headers cross an untrusted boundary. API-key mapping or a trusted upstream
must establish identity before an entitlement can elevate class. The admin plane is loopback/internal
by default. Provider URLs are static configuration, HTTPS outside loopback, and can be host-allowlisted.
Valkey holds ephemeral leases, not payloads. Observability exports metadata only.

Single-replica mode keeps scheduler, bounded request metadata, and capacity reservations in memory.
HA replicas use atomic expiring leases; coordinator failure is fail-closed in enforcement mode.
Running requests drain during shutdown and streaming remains unbuffered after upstream dispatch.
