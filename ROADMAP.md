# InferQoS roadmap

InferQoS is building a durable QoS layer, not another feature checklist for an AI gateway. Priorities
are selected by one question: **does this improve safe, measurable use of finite inference capacity
across providers?** Dates are intentionally not promised until a milestone is staffed and measured.

## Current product — 0.2.x

The current line is usable end to end:

- one Rust binary and zero-dependency single-instance mode;
- transparent streaming proxy and OpenAI-compatible insertion;
- fair, deadline-aware, work-aware scheduling with tenant/application policy;
- shadow mode, historical replay, synthetic workloads, and HTML/JSON reports;
- Azure, AWS, GCP, generic, fake, and external gRPC provider adapters;
- memory and atomic lease-based Valkey coordination;
- OIDC, mTLS, API-key, and trusted-proxy identity;
- OTLP/Prometheus telemetry and a private operational dashboard;
- Docker, Helm, ACA, ECS/Fargate, Cloud Run, systemd, signed releases, and multi-arch images.

Patch releases focus on correctness, security, documentation, deployment verification, and evidence.
No new public protocol becomes stable merely because it appears in a 0.x build.

## Now — prove behavior in real operating conditions

1. **Public benchmark corpus.** Publish reproducible hardware, workloads, raw outputs, scheduler
   baselines, proxy/streaming overhead, coordinator latency, fairness, and SLO attainment.
2. **Field calibration.** Evaluate sanitized provisioned-capacity traces across providers; improve
   token/work estimators and safety-factor convergence without provider monitoring in the hot path.
3. **Deployment confidence.** Expand credential-backed ephemeral cloud tests, upgrade/rollback
   tests, multi-zone failure tests, and published resource-sizing measurements.
4. **Operator clarity.** Stabilize configuration examples, alert packs, dashboard explanations,
   decision traces, and shadow-to-enforcement review checklists.
5. **Security assurance.** Add independent Rust/security review, public threat-model tracking,
   protocol fuzzing findings, and regular disaster exercises.

### Exit criteria

- Benchmarks are reproducible by a third party and regressions are gated without flaky PR checks.
- At least three distinct sanitized workload shapes have published replay assumptions and outcomes.
- Every supported deployment target has an automated manifest validation and a documented live test.
- No high-severity open security finding; supported upgrade and rollback paths are exercised.

## Next — stabilize protocols and capacity intelligence

- Promote the QoS header/configuration specification from `v1alpha1` only after compatibility rules,
  downgrade behavior, and gateway interoperability are proven.
- Version the external provider protocol with cross-version conformance fixtures and third-party
  adapter examples in at least two implementation languages.
- Improve per-pool capacity intelligence: uncertainty intervals, calibration history, saturation
  diagnosis, and clearer evidence/assumption separation in cost recommendations.
- Add pluggable token estimators and a bounded estimator-asset cache without making proprietary
  tokenizer APIs a hot-path dependency.
- Expand durable job integrations only where they remain outside realtime admission semantics.
- Publish APIM, Kong, and Envoy integration validation suites rather than configuration snippets
  that can silently rot.
- Make configuration migrations explicit, machine-checkable, and reversible before introducing a
  second schema version.

### Exit criteria

- Public compatibility matrix and deprecation window exist for every versioned surface.
- Third-party provider adapters pass the same conformance kit as built-in adapters.
- Capacity recommendations expose measured error and never imply guaranteed savings.
- An operator can upgrade one minor line with zero application changes and a documented rollback.

## Later — ecosystem scale without control-plane sprawl

- Community-maintained provider adapters and an interoperability registry based on test evidence.
- Larger open workload/benchmark corpus covering regulated, multilingual, long-context, and mixed
  batch/interactive patterns without storing prompt content.
- Federated capacity intelligence across explicitly configured pools while keeping routing
  deterministic and capacity/policy based.
- Independently released language SDKs only when the criteria in
  [ADR 0013](docs/adr/0013-sdk-repository-strategy.md) are met.
- A cautiously standardized neutral QoS protocol informed by production integrations rather than a
  vendor-prefixed header rename.

## Always out of scope

Semantic model routing, prompt management/storage, agent orchestration, generic API management,
content moderation, vector search/RAG, secrets management, GPU scheduling, model serving, and a
hosted telemetry/control-plane dependency are not roadmap items. See
[non-goals](docs/product/non-goals.md).

## How priorities change

Open an RFC when a proposal changes a public protocol, scheduler invariant, trust boundary, state
model, or operational dependency. Bugs, provider conformance gaps, measured performance regressions,
and security issues outrank speculative features. Roadmap movement requires an owner, acceptance
criteria, tests, documentation, and compatibility impact—not a marketing deadline.
