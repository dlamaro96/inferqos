---
title: "Provider notes"
---

# Provider notes

- Azure OpenAI: configure the provisioned deployment endpoint. With `auth: { type: ambient }`,
  workload identity is selected when `AZURE_FEDERATED_TOKEN_FILE` exists and managed identity
  otherwise; a short-lived cognitive-services token is fetched for each dispatch through the Azure
  SDK. 429 `retry-after-ms`/`retry-after` are feedback. API-key and explicit bearer modes also exist.
- AWS Bedrock: use the provisioned model ARN. Bedrock `InvokeModel`/streaming permissions belong to
  the task role. `AWS_BEARER_TOKEN_BEDROCK` is supported for configured Bedrock API-key flows; IAM
  `auth: { type: ambient }` loads the official AWS default credential/region chain and signs every
  request natively with SigV4 for the `bedrock` service. Task roles, IRSA, IAM Identity Center, and
  local profiles therefore remain AWS concerns; InferQoS stores no AWS key.
- Vertex AI: configure the provisioned-throughput endpoint and `auth: { type: ambient }`. The Google
  authentication library resolves ADC and refreshes cloud-platform bearer metadata per request,
  supporting service-account/workload-identity deployments without JSON keys. Be explicit about
  spillover behavior; InferQoS never assumes hidden PAYG failover.
- Generic: any HTTPS OpenAI-compatible finite endpoint, arbitrary model fields, streaming, custom
  auth header, and host allowlist.

Provider monitoring must never be called synchronously in the request hot path. Adapters translate
vendor work/capacity semantics into normalized units; the scheduler stays vendor-neutral.

Official binaries include `cloud-auth`. Custom minimal builds can disable default features; those
builds reject `ambient` configuration visibly rather than silently sending an unsigned request.
