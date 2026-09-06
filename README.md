# InferQoS

[![CI](https://github.com/dlamaro96/inferqos/actions/workflows/ci.yml/badge.svg)](https://github.com/dlamaro96/inferqos/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/dlamaro96/inferqos)](https://github.com/dlamaro96/inferqos/releases)
[![License](https://img.shields.io/github/license/dlamaro96/inferqos)](LICENSE)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/dlamaro96/inferqos/badge)](https://securityscorecards.dev/viewer/?uri=github.com/dlamaro96/inferqos)
[![Container](https://img.shields.io/badge/GHCR-multi--arch-2496ed)](https://github.com/dlamaro96/inferqos/pkgs/container/inferqos)
[![Docs](https://img.shields.io/badge/docs-inferqos-315fe8)](https://web.inferqos-website.daniamaro96.apps.pluglayer.io/docs/)

[Website](https://web.inferqos-website.daniamaro96.apps.pluglayer.io/) ·
[Live traffic lab](https://web.inferqos-website.daniamaro96.apps.pluglayer.io/demo/) ·
[Documentation](https://web.inferqos-website.daniamaro96.apps.pluglayer.io/docs/) ·
[Verified releases](https://github.com/dlamaro96/inferqos/releases)

**The open-source QoS control plane for finite and contracted AI inference capacity.**

You bought a finite block of AI inference capacity. Bursts now cause throttling, interactive work
gets stuck behind batch jobs, or you overprovision the next expensive block. InferQoS makes that
capacity behave like a shared, schedulable enterprise resource: applications ask for a service
class and deadline; policy, fairness, and predicted work decide what runs now.

[![InferQoS product overview](docs/assets/screenshots/overview.jpg)](https://web.inferqos-website.daniamaro96.apps.pluglayer.io/)

## Understand it in 30 seconds

| Without a QoS control plane | With InferQoS |
|---|---|
| A 100k-token batch request looks like one request | Admission is based on normalized estimated work |
| FIFO lets large jobs block urgent work | Deadlines protect realtime and interactive requests |
| Strict priority starves background work | Weighted fairness, aging, and guarantees keep every class moving |
| Each replica guesses at shared capacity | Valkey-backed leases coordinate HA replicas safely |
| Capacity planning depends on production changes | Shadow mode and replay project the effect before enforcement |

InferQoS **is not an AI gateway**. It does not inspect prompts to choose a model, store prompts,
manage agents, or replace APIM, Kong, or Envoy. A gateway answers **where should this request go?**
InferQoS answers **should this request consume scarce contracted capacity now?**

![InferQoS request path and trust boundaries](docs/assets/architecture.svg)

## See it work now

### Option A — browser, no installation

Open the [live traffic lab](https://web.inferqos-website.daniamaro96.apps.pluglayer.io/demo/).
Launch the burst and compare the same workload under uncontrolled admission and InferQoS. Select
Azure, AWS, Google Cloud, or a generic endpoint; then edit the purchased capacity, token mix,
cache rate, request rate, and latency. The translator shows weighted TPM, requests/minute, active
generations, and engaged-user estimates before applying the same workload to the scheduler. It is a safe browser simulation:
it uses no provider credentials and sends no prompt content.

[![InferQoS traffic lab](docs/assets/screenshots/traffic-lab.jpg)](https://web.inferqos-website.daniamaro96.apps.pluglayer.io/demo/)

### Option B — real local data plane, zero API keys

Requirements: Git, Docker Engine 24+ with Compose v2, and about 1 GB of free memory.

```bash
git clone https://github.com/dlamaro96/inferqos.git
cd inferqos
docker compose -f deploy/docker/compose.yaml up
```

Open `http://localhost:9090/ui`, then stream through the actual Rust proxy and deterministic fake
finite-capacity provider:

```bash
curl http://localhost:8080/v1/chat/completions \
  -H 'content-type: application/json' \
  -H 'authorization: Bearer local-demo-interactive' \
  -H 'x-inferqos-class: interactive' \
  -d '{"model":"fake","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

The default path pulls pinned, signed, multi-architecture release images. Contributors can add
`--build` or run `just demo-build` to compile the local checkout. `just demo` starts the released
stack. In a second terminal, `scripts/demo-load.sh` drives interactive, workflow, and
batch contention so you can watch interactive protection, batch progress, and full utilization.

### Insert it into an existing client

No SDK is required. Change the base URL and, when useful, add QoS headers:

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8080/v1",
    api_key="local-demo-interactive",
)

response = client.responses.create(
    model="configured-upstream-model",
    input="Summarize the incident timeline",
    extra_headers={
        "X-InferQoS-Class": "interactive",
        "X-InferQoS-Deadline-Ms": "3000",
    },
)
```

The client requests a class; authenticated application policy determines the **effective** class.
A caller cannot obtain unlimited priority by sending `realtime`.

## Choose a deployment

InferQoS itself is intentionally small. The numbers below are starting points for the control
plane—not the cost of inference capacity, network egress, logs, load balancers, private endpoints,
or optional managed Valkey. Prices vary by region and agreement; use the linked calculators before
production approval.

| Where | What you need | Checked-in starting size | Approximate InferQoS compute |
|---|---|---:|---:|
| Existing laptop/VM | Docker or one release binary | 1 replica | **$0 incremental** on existing compute |
| Kubernetes | Helm 3, cluster, config Secret | 2 replicas; 100m CPU/128 MiB requested each | Existing-cluster allocation only; a new cluster is rarely cost-effective just for InferQoS |
| Azure Container Apps | `az` login, resource group | 0.25 vCPU/0.5 GiB; 1 cost profile or 2 HA | The monthly free grant offsets some low activity; warm/active usage is metered by region |
| AWS ECS/Fargate | `aws` login, VPC, private subnets, Secrets Manager config | 0.25 vCPU/0.5 GB; 2 tasks | About **$9/task-month**, or **$18/month for two**, in us-east-1 before surrounding services |
| Google Cloud Run | `gcloud` login, project, service account, Secret Manager config | 1 vCPU/512 MiB; 2 warm instances | Roughly **$10–66/warm-instance-month**, depending mostly-idle vs active time and free allowance |
| systemd | Linux host, root for install only | 1 process | **$0 incremental** on an existing host |

Full assumptions, current official price sources, HA costs, network requirements, and a sizing
worksheet are in the [deployment guide](https://web.inferqos-website.daniamaro96.apps.pluglayer.io/docs/deployment/).

```bash
# Inspect everything first; this performs no deployment.
inferqos deploy --target kubernetes --config inferqos.yaml --dry-run

# Then use one target command after normal cloud authentication.
inferqos deploy --target docker --config inferqos.yaml
inferqos deploy --target aca --resource-group qos-production --config inferqos.yaml
inferqos deploy --target kubernetes --config inferqos.yaml
inferqos deploy --target ecs --region us-east-1 --vpc-id vpc-... \
  --subnets subnet-a,subnet-b --config-secret arn:aws:secretsmanager:... --config inferqos.yaml
inferqos deploy --target cloud-run --project my-project --region us-central1 \
  --config-secret inferqos-config --config inferqos.yaml
```

Cloud commands use your existing `az`, `aws`, or `gcloud` session. InferQoS uses managed identity,
task roles, or service accounts and does not require stored long-lived cloud keys. Admin ingress is
private by default.

## What operators control

One strict, versioned YAML file controls behavior. Unknown fields fail validation. Secrets are
referenced from environment variables or mounted secret files rather than stored in YAML.

| Concern | What an administrator can configure | Safe behavior |
|---|---|---|
| Rollout | `shadow` or `enforce` | Real-provider initialization defaults to shadow |
| Service | class weights, default deadlines, max queue time, per-class queue depth | Built-ins: realtime, interactive, standard, workflow, batch |
| Fairness | tenant/app weight, guaranteed share, max share | Idle shares are borrowable; guarantees recover under contention |
| Entitlements | classes and pools allowed for every application | Requested class can be downgraded or rejected |
| Isolation | tenant and application concurrency | Capacity and queue guards prevent monopolization |
| Identity | OIDC claims, mTLS certificate/SAN mappings, constant-time API key mappings, trusted proxies | Client identity headers are ignored outside configured trusted networks |
| Capacity | provider, endpoint, model/deployment, units, safety factor, host allowlist | Static targets and allowlists reduce SSRF risk |
| HA | memory or Valkey coordinator, expected replica count, lease TTL | Unsafe multi-replica memory mode is rejected at startup |
| Backpressure | body limit, memory/spool threshold, total queue depth/bytes | Secure bounded spool; overload fails visibly |
| Operations | admin binding/auth, decision history, config reload interval | Admin defaults to loopback; bad reload retains known-good config |
| Telemetry | Prometheus plus optional OTLP through standard environment variables | No prompts, secrets, raw user IDs, or project analytics |

Start with `inferqos init`, read the annotated [configuration reference](docs/reference/configuration.md),
then make validation part of delivery:

```bash
inferqos validate --config inferqos.yaml
inferqos policy test --config inferqos.yaml \
  --tenant finance --application treasury-assistant --class realtime
inferqos doctor --config inferqos.yaml
inferqos serve --config inferqos.yaml
```

Supported policy-only fields hot reload every two seconds by default. Invalid changes never replace
the last known-good policy. Listener addresses, TLS roots, coordinator type, pools, and OIDC key
sources require a graceful restart because changing a trust or capacity boundary in place is unsafe.

## A safer production adoption path

```text
deploy → shadow → observe → analyze/replay → review SLO impact → enforce
```

Shadow mode forwards requests immediately while recording metadata about what would have queued or
been rejected. Prompt and completion bodies are not recorded. Replay accepts JSONL, CSV, InferQoS
traces, or sanitized OpenTelemetry-derived metadata:

```bash
inferqos analyze traces.jsonl --capacity 50 \
  --capacity-increment 10 --cost-per-capacity-unit 120 \
  --json report.json --html report.html
```

Reports distinguish observed facts, projections, and assumptions. If demand is sustained and there
is not enough queueable work, InferQoS says additional capacity is required.

## Providers and extension points

| Provider | Authentication | Capacity feedback |
|---|---|---|
| Azure OpenAI / Foundry | API key or ambient managed/workload identity | usage reconciliation, 429, `retry-after`, `retry-after-ms` |
| AWS Bedrock | native SigV4 and AWS default credential chain | provisioned-capacity feedback, usage, throttling |
| Google Vertex AI | Application Default Credentials / workload identity | provisioned-throughput feedback, usage, throttling |
| OpenAI-compatible | bearer/API key/custom endpoint | configurable capacity model, usage, 429 learning |
| External adapter | Unix socket, loopback gRPC, or TLS/mTLS | versioned streaming protobuf contract |

Provider metrics are background calibration signals; no vendor monitoring API sits in the request
hot path. The [Provider SDK](docs/reference/provider-sdk.md), conformance kit, fake provider, and
[external protocol](docs/reference/external-provider-protocol.md) let new providers integrate
without changing the scheduler.

## SDK repository strategy

The Python, TypeScript, and Go helpers deliberately live in this monorepo today. They are small,
optional protocol helpers released with the server, so one change can update headers, examples,
tests, and compatibility in a single review. Splitting them now would create version drift and
three extra release/security pipelines without giving users more capability.

We will split a language SDK only when it has an independent maintainer and release cadence, a
substantial generated/admin client, or ecosystem tooling that materially requires its own repo.
The decision and migration guarantees are recorded in
[ADR 0013](docs/adr/0013-sdk-repository-strategy.md).

## Reliability, performance, and privacy

- Scheduling uses hierarchical weighted deficit fairness, normalized work estimates, deadline
  pressure, aging, deterministic tie-breaking, and non-preemptive admission.
- Single-instance mode needs no database or broker. HA uses atomic, expiring Valkey leases and
  fails closed when coordination cannot safely protect finite capacity.
- Streaming is preserved end to end; once upstream bytes have started, requests are not blindly
  replayed.
- Engineering budgets are p50 under 1 ms / p95 under 2 ms for scheduler decisions and p50 under
  5 ms / p95 under 10 ms incremental proxy overhead on modern commodity hardware. These are
  budgets, not an SLA; reproduce them with `inferqos benchmark` and `just benchmark`.
- Prompt logging, completion logging, body persistence beyond bounded temporary queue spooling,
  anonymous analytics, and phone-home telemetry are off.

Read the [scheduler model](docs/concepts/scheduling.md), [threat model](docs/security/threat-model.md),
[telemetry policy](docs/security/telemetry.md), and [operations runbook](docs/operations/runbook.md).

## Install a verified release

Download the installer before executing it. It never invokes `sudo`; it verifies the Sigstore
workflow identity for the checksum manifest, the artifact SHA-256, and—when `gh` is available—the
GitHub build attestation.

```bash
curl -fsSLO https://raw.githubusercontent.com/dlamaro96/inferqos/main/install.sh
less install.sh
sh install.sh
inferqos version
inferqos init
```

Release artifacts cover Linux amd64/arm64, macOS arm64/x86_64, Windows x86_64, and multi-architecture
OCI images. See [all releases](https://github.com/dlamaro96/inferqos/releases).

## CLI map

| Journey | Commands |
|---|---|
| Start and validate | `init`, `validate`, `doctor`, `serve`, `version` |
| Verify policy | `policy test`, `explain` |
| Operate capacity | `capacity status`, `shadow` |
| Evaluate value | `analyze`, `replay`, `benchmark` |
| Ship and maintain | `deploy`, `upgrade`, `diagnostics collect` |

## When not to use InferQoS

It is usually a poor fit when there is only one workload; inference is effectively unlimited PAYG
with no cost concern; capacity is permanently saturated; nothing can wait; the application cannot
tolerate any queueing; or the provider already offers equivalent business-aware QoS.

## Project direction and community

The [roadmap](ROADMAP.md) is organized around proof, protocol stability, capacity intelligence, and
ecosystem adoption—not generic gateway features. Apache-2.0 licensed. Contributions use DCO signoff.
Read [CONTRIBUTING.md](CONTRIBUTING.md), [GOVERNANCE.md](GOVERNANCE.md), and
[SUPPORT.md](SUPPORT.md). Report vulnerabilities through GitHub private vulnerability reporting,
never a public issue.
