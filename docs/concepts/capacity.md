---
title: "Capacity translation"
description: "Translate provider-native inference contracts into weighted tokens, request throughput, active generations, and workload pressure."
---

# Capacity translation

There is no portable provider capacity unit. Azure PTUs, Amazon Bedrock token or Model Unit
contracts, Google Vertex GSUs, and a self-hosted endpoint's measured throughput describe different
things. InferQoS therefore keeps four layers separate:

```text
provider-native purchase
        ↓ model/deployment coefficients
weighted token envelope
        ↓ representative request shape
requests per minute + active generations
        ↓ prompt rate per engaged user
human workload estimate
```

The scheduler uses normalized `WorkEstimate` values. The dashboard and reports must also display
the native contract and conversion assumptions so an operator can audit the number.

## Weighted request work

For a typical text request, the planning calculation is:

```text
weighted request tokens =
    uncached input tokens
  + cached input tokens × cache weight
  + expected output tokens × output weight
```

The coefficients belong to the selected model, version, deployment type, modality, and provider.
They are not universal constants. The resulting planning envelope is:

```text
supported requests/minute = weighted tokens/minute ÷ weighted tokens/request
active generations        = supported requests/minute × average latency seconds ÷ 60
engaged users              = supported requests/minute ÷ prompts per engaged user per minute
```

“Concurrent users” is not a provider unit. An open browser session consumes no inference capacity
while the person reads. InferQoS uses active generations for runtime concurrency and presents
engaged users only as an assumption-backed planning estimate.

## Provider inputs

| Provider | Native input | Required qualification |
|---|---|---|
| Azure OpenAI / Foundry | PTUs | Model, version, deployment type, prompt/output mix, and cache behavior |
| Amazon Bedrock | Contracted tokens or Model Units | Contract/model-specific input and output throughput; one MU is not portable |
| Google Vertex AI | GSUs | Model, modality, version, and published burndown weights |
| OpenAI-compatible finite endpoint | Measured weighted TPM | Observed latency, usage, burst behavior, and throttle feedback |

For example, Microsoft's current GPT-4o global/data-zone sizing example lists 2,500 input TPM per
PTU and an output token ratio of 4. With 50 PTUs, a typical request of 900 input and 180 output
tokens, 20% cached input, and a cache weight of zero:

```text
weighted envelope          = 50 × 2,500 = 125,000 weighted TPM
weighted request           = 900 × 80% + 180 × 4 = 1,440
estimated throughput       = 125,000 ÷ 1,440 ≈ 86 requests/minute
at 10 s average latency    = 86 × 10 ÷ 60 ≈ 14 active generations
at 0.25 prompts/user/min   = 86 ÷ 0.25 ≈ 347 engaged users
```

This is an explainable example, not a promise that 50 PTUs always provides that workload. Model
tables change. Load test representative traffic and calibrate in shadow mode before enforcement.

## Current primary references

- [Microsoft provisioned throughput sizing](https://learn.microsoft.com/azure/foundry/openai/how-to/provisioned-throughput-sizing)
- [Amazon Bedrock provisioned throughput](https://docs.aws.amazon.com/bedrock/latest/userguide/prov-throughput.html)
- [Google Vertex AI provisioned throughput measurement](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/provisioned-throughput/measure-provisioned-throughput)

The public [capacity translator](https://web.inferqos-website.daniamaro96.apps.pluglayer.io/demo/#capacity-translator)
implements these formulas with editable assumptions.
