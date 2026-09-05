# InferQoS

**The open-source QoS control plane for finite and contracted AI inference capacity.**

You bought finite AI inference capacity. Bursts cause throttling or force overprovisioning.
InferQoS decides which workloads should consume that capacity now, protects interactive traffic,
and queues less urgent work.

```text
Apps ──► existing gateway (optional) ──► InferQoS ──► finite AI capacity
                                             │
                           identity → estimate → fair admission
```

InferQoS is not an AI gateway. It does not choose models from prompt semantics, store prompts,
manage agents, or replace APIM/Kong/Envoy. A gateway answers “where should this go?” InferQoS
answers “should this consume scarce contracted capacity now?”

## Five-minute, zero-key demo

```bash
git clone https://github.com/dlamaro96/inferqos.git
cd inferqos
docker compose -f deploy/docker/compose.yaml up --build
```

Open `http://localhost:9090/ui`, then send an OpenAI-compatible request:

```bash
curl http://localhost:8080/v1/chat/completions \
  -H 'content-type: application/json' \
  -H 'x-inferqos-class: interactive' \
  -d '{"model":"fake","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

The same insertion works with standard clients:

```python
from openai import OpenAI
client = OpenAI(base_url="http://localhost:8080/v1", api_key="unused-in-demo")
```

New real-provider configurations default to shadow mode. No application rewrite is needed to
switch to enforcement.

## What is implemented

- Streaming reverse proxy for Responses, Chat Completions, Embeddings, and future transparent paths
- Hierarchical weighted deficit fairness using estimated work, deadline pressure, and queue aging
- Built-in `realtime`, `interactive`, `standard`, `workflow`, and `batch` classes
- Azure OpenAI, AWS Bedrock, Google Vertex AI, generic OpenAI-compatible, and fake HTTP adapters
- Local adaptive capacity ledger and atomic lease-based Valkey coordinator
- Strict policy/config schema, entitlement downgrade, bounded queues, decision explanations
- Shadow metrics, JSONL/CSV replay, terminal/JSON/HTML reports, and honest capacity recommendations
- Prometheus endpoint, structured logs, health/readiness, and embedded prompt-free dashboard
- Docker, Helm, ACA, ECS/Fargate, Cloud Run, and systemd deployment assets
- Lightweight Python, TypeScript, and Go header helpers

No database, broker, Kubernetes cluster, SDK, hosted control plane, or telemetry account is required.

## Install and deploy

Download the installer before executing it; it never invokes `sudo` and verifies the published
SHA256 manifest:

```bash
curl -fsSLO https://raw.githubusercontent.com/dlamaro96/inferqos/main/install.sh
sh install.sh
inferqos init
inferqos doctor
inferqos deploy --target docker
```

Cloud commands use your normal `az`, `aws`, or `gcloud` authentication. InferQoS does not store
long-lived cloud credentials.

## CLI

`serve`, `init`, `validate`, `doctor`, `policy test`, `capacity status`, `analyze`, `replay`,
`shadow`, `benchmark`, `deploy`, `upgrade`, `version`, `explain`, and `diagnostics collect` are
available from the single `inferqos` binary.

## Performance and claims

Run `inferqos benchmark` for scheduler-only overhead and `just benchmark` for repeatable scenarios.
The engineering budgets are p50 under 1 ms / p95 under 2 ms for scheduling, and p50 under 5 ms /
p95 under 10 ms incremental proxy overhead on modern commodity hardware. These are budgets—not an
SLA. Published numbers must include hardware and methodology. Replay outputs are projections and
never guaranteed savings.

## Security and privacy

Prompt logging, completion logging, request persistence, anonymous analytics, and phone-home
telemetry are off. The admin listener defaults to loopback. Queues and bytes are bounded, upstreams
are static and optionally host-allowlisted, client QoS headers are entitlement-checked, and secrets
are read from environment variables. See the [threat model](docs/security/threat-model.md) and
[telemetry policy](docs/security/telemetry.md).

## When not to use InferQoS

It is usually a poor fit for one workload, unlimited PAYG with no cost concern, permanently
saturated capacity, no queueable work, an application that cannot tolerate any queueing, or a
provider already offering equivalent business-aware QoS.

## Project

Apache-2.0 licensed. Contributions use DCO sign-off. Read [CONTRIBUTING.md](CONTRIBUTING.md),
[GOVERNANCE.md](GOVERNANCE.md), and the [roadmap](ROADMAP.md). Security reports should use GitHub
private vulnerability reporting, not a public issue.

