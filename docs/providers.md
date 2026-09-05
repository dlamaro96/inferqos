# Provider notes

- Azure OpenAI: configure the provisioned deployment endpoint. 429 `retry-after-ms`/`retry-after`
  are feedback; Azure Monitor is background calibration only. API key and bearer-token modes exist.
- AWS Bedrock: use the provisioned model ARN. Bedrock `InvokeModel`/streaming permissions belong to
  the task role. `AWS_BEARER_TOKEN_BEDROCK` is supported for configured Bedrock API-key flows; IAM
  SigV4 SDK transport is the preferred workload-identity deployment path.
- Vertex AI: configure the provisioned-throughput endpoint and workload identity/ADC. Be explicit
  about spillover behavior; InferQoS never assumes hidden PAYG failover.
- Generic: any HTTPS OpenAI-compatible finite endpoint, arbitrary model fields, streaming, custom
  auth header, and host allowlist.

Provider monitoring must never be called synchronously in the request hot path. Adapters translate
vendor work/capacity semantics into normalized units; the scheduler stays vendor-neutral.

