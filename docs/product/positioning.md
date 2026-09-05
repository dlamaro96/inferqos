---
title: "Positioning"
---

# Positioning

InferQoS is QoS, admission control, and capacity intelligence for finite inference. It complements
API gateways, LLM gateways, and provider endpoints. It does not perform semantic routing, prompt
management, authentication governance, or model serving.

An existing gateway decides where a request is routed. A GPU serving scheduler decides which local
GPU executes it. InferQoS decides whether the request should consume scarce contracted inference
capacity now, wait, or yield to another entitled workload.

## Adoption path

1. Deploy without changing application payloads.
2. Run shadow mode for representative days.
3. Analyze metadata and replay historical demand.
4. Review projected queue and SLO impact.
5. Change `mode: shadow` to `mode: enforce` with no application rewrite.

## When not to use it

Do not add InferQoS when traffic has only one class, capacity is effectively unlimited and cost is
irrelevant, demand is permanently saturated, no work may queue, or equivalent identity-aware QoS
already exists at the provider.
