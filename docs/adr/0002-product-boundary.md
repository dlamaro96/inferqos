---
title: "ADR 0002: QoS control plane, not an AI gateway"
---

# ADR 0002: QoS control plane, not an AI gateway

- Status: Accepted
- Date: 2026-09-05

InferQoS owns admission control, fair scheduling, finite-capacity accounting, and capacity
intelligence. It deliberately does not own semantic model routing, prompt management,
guardrails, authentication governance, model serving, or agent orchestration. Existing gateways
answer *where* a request is routed; InferQoS answers *whether scarce contracted capacity should
serve it now or another entitled workload first*.
