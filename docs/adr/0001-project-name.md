---
title: "ADR 0001: Project name"
---

# ADR 0001: Project name

- Status: Accepted
- Date: 2026-09-05

## Decision

Use **InferQoS** for the product and `inferqos` for the repository, binary, crate prefix,
container, and package family.

## Context

On 2026-09-05 we performed a pragmatic collision check against GitHub repository search,
the crates.io API/search surface, PyPI, npm, Docker Hub, general web search, and obvious
trademark-result searches. Exact-name searches found no existing project, package, image,
commercial product, or obvious registered mark. The name directly communicates inference
quality-of-service without implying model serving or a generic gateway.

This check is not a legal opinion or trademark clearance. Maintainers should repeat it before
material commercial adoption or registration.

## Consequences

All public interfaces use `InferQoS` or the lowercase `inferqos`. Vendor-prefixed protocol
headers begin with `X-InferQoS-` until an independent specification matures.
