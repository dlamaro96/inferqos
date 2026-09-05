---
title: "QoS request protocol"
---

# QoS request protocol

Maturity: **experimental vendor extension**, version 0.1. These headers are not an industry standard.

| Header | Meaning |
|---|---|
| `X-InferQoS-Class` | Requested `realtime`, `interactive`, `standard`, `workflow`, or `batch`. |
| `X-InferQoS-Deadline-Ms` | Maximum queue/admission budget in milliseconds, capped by policy. |
| `X-InferQoS-Tenant` | Tenant hint; ignored unless a trusted identity source establishes it. |
| `X-InferQoS-Application` | Application hint; ignored unless trusted. |
| `X-InferQoS-Queueable` | Whether this request may wait. |
| `X-InferQoS-Request-Id` | UUID used for decision correlation/idempotency metadata. |

Client input is a request, never an entitlement. The effective class derives from authenticated
principal, tenant, application, and configured policy. A request can be downgraded or rejected.
Provider calls cannot be promised exactly once across arbitrary upstream APIs.
