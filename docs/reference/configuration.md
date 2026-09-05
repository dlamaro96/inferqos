---
title: "Configuration reference"
description: "Configure InferQoS rollout, identity, service classes, fairness, capacity pools, HA, and operations safely."
---

# Configuration reference

InferQoS uses one strict YAML document. It is configuration-as-code: review it, validate it, store
it in Git or a secret manager, and deploy it through the same path as the binary. Unknown fields,
invalid references, unsafe HA, and invalid ranges fail instead of being ignored.

```yaml
apiVersion: inferqos.io/v1alpha1
kind: InferQoSConfig
mode: shadow
```

Run these checks before every rollout:

```bash
inferqos validate --config inferqos.yaml
inferqos policy test --config inferqos.yaml \
  --tenant finance --application treasury-assistant --class interactive
inferqos doctor --config inferqos.yaml
```

The generated JSON Schema is `config.schema.json`. Generate it with `just schema`; CI verifies the
checked-in schema remains current.

## Administrator control map

| Section | Controls | Typical owner |
|---|---|---|
| `mode` | Observation-only `shadow` or active `enforce` | Platform/SRE |
| `server` | Data listener, direct mTLS, request size, spool threshold/location, reload interval | Platform/security |
| `admin` | Private listener, bearer-token environment name, decision-history exposure | Platform/security |
| `coordinator` | In-process state or Valkey URL environment and lease TTL | Platform/SRE |
| `service_classes` | Weight, default deadline, max queue duration, per-class queued request cap | Product/SRE |
| `pools` | Provider, fixed endpoint, model/deployment/region, normalized capacity, auth, allowlist, safety factor | AI platform |
| `policies.tenants` | Weight, guaranteed/minimum share, maximum share, concurrency | Platform/business owner |
| `policies.applications` | Tenant membership, class entitlement, weight, concurrency, permitted pools | Application owner + platform |
| `policies.api_keys` | API key secret-environment-to-identity mappings | Security/platform |
| `identity` | OIDC issuer/claims, trusted proxy networks/headers, mTLS SAN and certificate mappings | Identity/security |
| `limits` | Total queue depth/bytes, decision ring size, expected replicas, unsafe-HA override | SRE |

## Working production-oriented example

Secrets are environment substitutions, never literal values. This example uses a generic finite
OpenAI-compatible endpoint; select `azure-openai`, `aws-bedrock`, `gcp-vertex`, `fake`, or
`external-grpc` for another adapter.

```yaml
apiVersion: inferqos.io/v1alpha1
kind: InferQoSConfig
mode: shadow

server:
  listen: 0.0.0.0:8080
  max_body_bytes: 16777216
  spool_threshold_bytes: 262144
  spool_directory: /tmp/inferqos-spool
  config_reload_interval: 2s

admin:
  listen: 127.0.0.1:9090
  bearer_token_env: INFERQOS_ADMIN_TOKEN
  expose_decisions: false

coordinator:
  type: memory

service_classes:
  realtime:    { weight: 100, default_deadline: 500ms, max_queue: 100ms, max_queued: 100 }
  interactive: { weight: 50,  default_deadline: 3s,    max_queue: 3s,    max_queued: 1000 }
  standard:    { weight: 20,  default_deadline: 10s,   max_queue: 10s,   max_queued: 3000 }
  workflow:    { weight: 10,  default_deadline: 30s,   max_queue: 60s,   max_queued: 3000 }
  batch:       { weight: 1,   default_deadline: 30m,   max_queue: 30m,   max_queued: 3000 }

pools:
  primary:
    provider: openai-compatible
    endpoint: ${INFERQOS_UPSTREAM}
    model: null
    deployment: null
    region: null
    capacity_units: 50000
    auth: { type: bearer, env: INFERQOS_UPSTREAM_API_KEY }
    allowed_hosts: [inference.internal.example]
    initial_safety_factor: 1.15

policies:
  tenants:
    finance: { weight: 3, guaranteed_share: 0.30, max_share: 0.70, max_concurrency: 80 }
    corporate: { weight: 1, guaranteed_share: 0.10, max_share: 1.0, max_concurrency: 100 }
  applications:
    treasury-assistant:
      tenant: finance
      allowed_classes: [realtime, interactive, standard]
      weight: 2
      max_concurrency: 30
      permitted_pools: [primary]
  api_keys: {}

identity:
  oidc:
    issuer: https://identity.example/
    audience: inferqos
    principal_claim: sub
    tenant_claim: tenant
    application_claim: application
    required: true
  trusted_proxy_cidrs: []
  trusted_headers:
    principal: x-inferqos-principal
    tenant: x-inferqos-tenant
    application: x-inferqos-application
    client_cert_san: x-forwarded-client-cert-san
  mtls_san_mappings: {}
  mtls_certificate_sha256_mappings: {}

limits:
  total_queue_depth: 10000
  total_queue_bytes: 268435456
  decision_history: 2048
  expected_replicas: 1
  allow_unsafe_uncoordinated_ha: false
```

## Service classes

All five built-ins must exist. `weight` controls relative scheduler service, not a fixed reservation.
`default_deadline` applies when the request does not supply one. `max_queue` is the hard waiting
budget. `max_queued` bounds per-class metadata and bodies. Durations accept values such as `100ms`,
`3s`, and `30m`.

Guaranteed tenant shares remain borrowable. If batch is the only work, it may use the whole pool.
When competing work appears, new admission restores the configured guarantees; InferQoS does not
pretend it can pause an already-running provider call.

## Identity and entitlement

Identity may come from a validated OIDC bearer token, a directly verified mTLS certificate, a
constant-time API key mapping, or headers from a configured trusted proxy network. Trusted identity
headers are stripped or ignored for all other sources. The resolution is:

```text
requested class + authenticated identity + tenant/application policy = effective class
```

`allowed_classes` is an entitlement boundary. `permitted_pools` prevents applications from
selecting pools they do not own. Empty permitted-pool sets mean the policy does not further narrow
the configured pool set. See [identity and trust](../security/identity.md) before enabling proxy
headers.

## Capacity pools and authentication

Each pool has a provider-owned endpoint and normalized `capacity_units`. A request cannot provide
an arbitrary upstream URL. `allowed_hosts` is an additional static SSRF control and should be set in
production. `initial_safety_factor` must be between 1 and 4; online calibration adjusts cautiously
as actual usage and throttling arrive.

Authentication types:

- `none` for a local fake or otherwise protected endpoint;
- `api-key` with an environment variable and configurable header;
- `bearer` with an environment variable;
- `ambient` for Azure managed/workload identity, AWS default credential chain with SigV4, or GCP
  Application Default Credentials.

Provider 429s, retry-after hints, and actual usage reconcile reservations. Provider monitoring is a
background calibration source and is never fetched synchronously in the hot path.

## HA coordinator

Single-instance mode uses `type: memory`. Replicas sharing one finite pool must use Valkey:

```yaml
coordinator:
  type: valkey
  url_env: INFERQOS_VALKEY_URL
  lease_ttl: 30s
limits:
  expected_replicas: 3
  allow_unsafe_uncoordinated_ha: false
```

Reservations and leases are atomic and expire after a crashed replica disappears. Coordinator
failure is fail-closed in enforce mode because protecting the finite pool is safer than flooding it.
Valkey stores ephemeral capacity metadata, not prompts or response bodies.

## Queue memory and disk

Bodies at or below `spool_threshold_bytes` may wait in bounded memory. Larger queueable bodies are
written with exclusive creation into an owner-only spool directory and removed after use,
cancellation, or rejection. `max_body_bytes`, `total_queue_depth`, and `total_queue_bytes` provide
hard overload boundaries. Mount an encrypted, bounded ephemeral volume in production. Spooling is
not durable job storage.

## Hot reload versus restart

InferQoS parses and semantically validates every candidate before replacing live state. A bad file
is logged and the last known-good configuration remains active.

| Change | Live reload | Why |
|---|---:|---|
| Service-class weights, deadlines, queue limits | Yes | Scheduler policy state can be replaced safely |
| Tenant/application policy and identity mappings | Yes | Existing running calls remain non-preemptive |
| Queue/history limits | Yes | New admissions observe the new bound |
| Listener addresses or direct TLS roots | No | Network trust boundary requires listener restart |
| Coordinator type/URL/lease semantics | No | Avoid split coordination domains |
| Capacity pools/provider endpoints | No | Avoid moving in-flight reservations between ledgers |
| OIDC issuer/JWKS source | No | Trust-root transition requires controlled restart |

Use a rolling update for restart-required changes. Validate the new document first, keep the prior
Git revision, wait for readiness, and allow the old replica's 30-second drain window.

## Environment and secret rules

- `${NAME}` is expanded from the process environment. Missing variables fail startup.
- Variable names may contain only ASCII letters, digits, and underscore.
- Provider keys and coordinator URLs belong in environment variables or mounted secret sources.
- `INFERQOS_CONFIG_YAML` can supply the whole document when a configured file is absent; this is
  useful for cloud secret injection.
- `OTEL_EXPORTER_OTLP_ENDPOINT` enables OTLP/gRPC traces and metrics. Standard OpenTelemetry header
  and TLS environment variables remain available.
- `RUST_LOG` controls structured log filtering.

The canonical starting file is [`inferqos.example.yaml`](../../inferqos.example.yaml).
