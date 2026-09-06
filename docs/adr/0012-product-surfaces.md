---
title: "ADR 0012: Separate product, operations, and simulation surfaces"
---

# ADR 0012: Separate product, operations, and simulation surfaces

## Status

Accepted.

## Context

InferQoS needs three different user experiences with different security and performance boundaries:

1. a public product and documentation entry point;
2. an operator dashboard in the management plane;
3. a safe demonstration of scheduling behavior before deployment.

Coupling a marketing frontend to the Rust data plane would increase hot-path dependencies and could
encourage public exposure of the admin listener. Rendering a fake dashboard would also weaken trust:
operators need live data, while evaluators need a reproducible model with explicit assumptions.

## Decision

- The embedded operations dashboard remains dependency-free assets compiled into the Rust binary.
  It reads only versioned management APIs, has no external resources, and inherits management-plane
  authentication and CSP controls.
- The public website and canonical reader-facing documentation are a separate static, unprivileged
  OCI workload. It never reads customer operational APIs and contains no analytics. Its source and deployment lifecycle live
  in a separate private repository so the OSS product tree remains focused on the distributable
  control plane, embedded operational UI, documentation, and integration contracts.
- Documentation source remains alongside the code in this public repository for review, release
  versioning, and offline use. The product website renders the canonical navigation and reading
  experience; repository Markdown and GitHub Pages are source/fallback surfaces, not the primary URL.
- `/demo/` is a deterministic browser-only educational simulation. It compares admission policies
  against the same generated metadata and clearly states that it is not a provider benchmark.
- Historical analysis intended for real capacity decisions remains in `inferqos analyze` and
  `inferqos replay`; the browser demo does not replace them.
- Production hosting for the public site and runtime validation use separate deployment projects.

## Consequences

The data plane remains small and deployable without Node.js or a marketing frontend build. The
public site can be deployed and scaled independently. Operators do not need an external dashboard
dependency, and the public demo cannot access prompts, credentials, tenant identities, or runtime
state. Public releases do not contain or build the website source.
