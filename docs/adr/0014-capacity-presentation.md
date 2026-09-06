---
title: "ADR 0014: Provider-native and human-readable capacity presentation"
---

# ADR 0014: Provider-native and human-readable capacity presentation

## Status

Accepted.

## Context

An unlabeled normalized value such as “100 units” is useful inside a deterministic scheduler test
but meaningless to an operator. Provider capacity products use non-equivalent units, and their
throughput changes by model, version, deployment type, modality, token mix, and cache behavior.
Conversely, “concurrent users” sounds accessible but is not a stable capacity measure because user
prompt frequency and generation latency vary.

## Decision

Every operator-facing capacity surface presents four distinct layers:

1. the provider-native purchase or measured limit;
2. a model-specific weighted-token planning envelope;
3. estimated request throughput and active generations from an editable representative request;
4. an optional engaged-user estimate with its prompt-frequency assumption.

Internal scheduler work units remain implementation details. Reports retain each coefficient,
source, confidence, and timestamp. Provider presets are examples sourced from current official
documentation, never silent universal conversions. Shadow mode and representative load tests are
the validation path.

## Consequences

Capacity values take more space but become auditable and portable. A cloud-specific contract does
not leak into scheduler APIs. Human-scale estimates remain useful without being mistaken for a
guarantee, and InferQoS can honestly distinguish short queueable peaks from sustained saturation.
