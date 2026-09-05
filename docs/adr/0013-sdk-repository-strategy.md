---
title: "ADR 0013: SDK repository strategy"
---

# ADR 0013: Keep optional SDK helpers in the monorepo until autonomy is real

- Status: accepted
- Date: 2026-09-05

## Context

InferQoS clients do not require an SDK: changing the OpenAI-compatible base URL and adding headers
is sufficient. The current Python, TypeScript, and Go packages are intentionally small convenience
helpers for QoS metadata and management calls. Their public behavior is coupled to the versioned QoS
and admin protocols.

Separate repositories can eventually improve language-specific ownership and release velocity, but
they immediately multiply CI, dependency updates, security policies, release credentials, issue
triage, compatibility testing, and contributor discovery. Repositories are not an architecture;
premature splitting would make protocol drift more likely.

## Decision

Keep all official SDKs under `sdk/` in the public InferQoS monorepo. Test every package in the main
compatibility workflow. Give packages independent ecosystem names and package versions, and support
language-specific tags/releases when publication begins.

Split one SDK into its own repository only when at least two of these are true:

1. it has a dedicated maintainer or reviewer group;
2. it needs releases materially more often than the server;
3. it contains a substantial generated management client or language-native integration surface;
4. its ecosystem requires tooling that makes monorepo publication unreliable;
5. issue volume and contributors are independently sustainable;
6. its compatibility matrix no longer fits the main repository CI budget.

Any split must preserve package coordinates, history, license, DCO, vulnerability reporting,
protocol compatibility tests, and links from the main repository. The monorepo remains the source
of truth for protocol definitions.

## Consequences

Protocol and helper changes stay atomic today, users have one discovery surface, and supply-chain
policy remains centralized. Language packages can still publish independently from subdirectories.
At larger scale, a deliberate split remains possible without changing the proxy integration model.
