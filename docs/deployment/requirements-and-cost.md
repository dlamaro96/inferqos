---
title: "Requirements and cost"
description: "Choose an InferQoS deployment using explicit prerequisites, sizing, HA, and cost assumptions."
---

# Deployment requirements and cost

InferQoS has no license fee and needs no hosted InferQoS service. The meaningful cost is the small
control-plane runtime plus whatever infrastructure you choose around it. **Inference capacity is
not included** in the estimates below; neither are network egress, observability ingestion, load
balancers, private endpoints/NAT, secret stores, or optional managed Valkey.

These are planning estimates, not quotes. Prices and free allowances change by region, account,
currency, and agreement. The source rates below were checked on **2026-09-05**; run the provider
calculator before approving production spend.

## Choose by environment

| Target | Minimum practical prerequisites | Checked-in profile | HA and coordination | Runtime-cost guidance |
|---|---|---|---|---|
| Docker | Docker Engine 24+, Compose v2 for demo, 1 GB free RAM | One container; admin bound to loopback | Run one replica with memory coordinator; use Valkey before sharing one pool across replicas | $0 incremental on an existing host |
| systemd | 64-bit Linux, systemd, root only during installation, outbound TLS to provider | One unprivileged process, read-only config | Use a load balancer plus Valkey for multiple hosts | $0 incremental on an existing host; otherwise the chosen VM price |
| Kubernetes | Kubernetes 1.27+, Helm 3, config Secret, outbound provider access | 2 pods; request 100m CPU/128 MiB, limit 1 CPU/512 MiB each | PDB, HPA, topology spread; Valkey required for a shared pool | Existing-cluster allocation only; do not create a cluster solely for this small service |
| Azure Container Apps | Azure CLI, authenticated subscription, resource group, permission for Container Apps, identity, and Log Analytics resources | Cost: 1 × 0.25 vCPU/0.5 GiB; HA: 2–10 × 0.5 vCPU/1 GiB | Use private/internal ingress and Valkey when replicas share a pool | Consumption is billed by active/idle vCPU, memory, and requests; calculate by region |
| AWS ECS/Fargate | AWS CLI, task-role permissions, VPC, at least two private subnets for HA, Secrets Manager config, outbound provider path | 2 × 0.25 vCPU/0.5 GB tasks | Rolling deployment across subnets; add Valkey/ElastiCache for shared-pool correctness | About $9/task-month, or $18/month for two in us-east-1, compute only |
| Google Cloud Run | gcloud CLI, enabled Cloud Run/Artifact Registry/Secret Manager APIs, service account, internal caller path | 2–10 × 1 vCPU/512 MiB warm instances | Warm instances and bounded concurrency; use Valkey for a shared pool | About $10 mostly-idle to $66 continuously active per warm instance before free allowance |

## Where the estimates come from

### Azure Container Apps

The checked-in Bicep profile uses the smallest Consumption combination, 0.25 vCPU and 0.5 GiB,
for cost-optimized deployment; the HA profile uses two warm 0.5 vCPU/1 GiB replicas. Microsoft
documents valid Consumption combinations beginning at 0.25 vCPU/0.5 GiB and a monthly free grant
of 180,000 vCPU-seconds, 360,000 GiB-seconds, and two million requests. Active and idle rates are
region/agreement dependent, so this guide does not invent a dollar value when Microsoft's public
table does not resolve one for the operator's account.

- [Container Apps pricing](https://azure.microsoft.com/en-us/pricing/details/container-apps/)
- [Container resource combinations](https://learn.microsoft.com/azure/container-apps/containers#vcpus-and-memory-allocation-requirements)
- [Azure pricing calculator](https://azure.microsoft.com/en-us/pricing/calculator/)

### AWS ECS/Fargate

The template uses 0.25 vCPU and 0.5 GB. AWS's published Linux/x86 rate for us-east-1 is
$0.000011244 per vCPU-second and $0.000001235 per GB-second. For 730 hours:

```text
(0.25 × $0.000011244 + 0.5 × $0.000001235) × 2,628,000 seconds
= approximately $9.01 per task-month
```

The two-task HA template is therefore about $18.02/month in Fargate compute. This excludes NAT
gateway, public IPv4, load balancer, CloudWatch, Secrets Manager, transfer, and Valkey costs; those
can exceed the container compute and must be designed deliberately.

- [AWS Fargate pricing and examples](https://aws.amazon.com/fargate/pricing/)
- [AWS Pricing Calculator](https://calculator.aws/)

### Google Cloud Run

The manifest uses 1 vCPU and 512 MiB with two minimum instances. In us-central1, Cloud Run lists
request-based idle time at $0.0000025 per vCPU-second and $0.0000025 per GiB-second. A completely
idle warm instance is therefore approximately $9.86/month before the free allowance. The same
shape active for every second under request-based billing is approximately $66.36/month; under
instance-based billing it is approximately $49.93/month. Real traffic normally lands between idle
and active boundaries. Region, billing mode, requests, transfer, VPC connectivity, logs, and the
free allowance change the result.

- [Cloud Run pricing](https://cloud.google.com/run/pricing)
- [Cloud Run CPU and memory limits](https://cloud.google.com/run/docs/configuring/services/cpu)
- [Google Cloud pricing calculator](https://cloud.google.com/products/calculator)

## Size it without guessing

Start with the checked-in limits, run shadow mode, then change one dimension at a time:

| Signal | First response |
|---|---|
| Scheduler decision p95 approaches 2 ms | Add CPU or replicas; check coordinator latency before either |
| Resident memory approaches its limit | Lower queue bytes/history or add memory; do not allow swapping |
| Queue bytes approach 80% | Reduce queue limits, increase spool volume, or reject earlier |
| Coordinator latency rises | Keep Valkey near replicas; investigate saturation/network before adding schedulers |
| Provider is continuously saturated | Scheduling cannot create capacity; add capacity or make more work queueable |
| Runtime CPU is low but provider 429s rise | Fix pool capacity/calibration or provider limits, not runtime size |

For a single low-volume pool, begin with one replica and no external services. For availability or
horizontal scale, use at least two replicas and Valkey with the pool close enough that reservation
latency stays small. `limits.expected_replicas > 1` with the memory coordinator fails validation
unless the operator explicitly accepts unsafe isolated-pool behavior.

## Production network checklist

- Keep the admin listener on loopback or private/internal ingress and require a bearer token when
  it is remotely reachable.
- Place replicas and Valkey in the same low-latency trust boundary. Valkey contains ephemeral lease
  metadata, never prompts.
- Permit egress only to configured provider, coordinator, identity, and OTLP endpoints.
- Use managed/workload identity, task roles, or service accounts instead of long-lived cloud keys.
- Budget explicitly for private networking. An always-on NAT gateway can cost more than InferQoS.
- Mount configuration from a managed secret or immutable deployment artifact and validate it before
  rollout.
- Use two availability zones where the platform supports it, readiness probes, and a 30-second drain
  window.

## Preview before spending

Every deployment command supports `--dry-run`; it validates the InferQoS configuration and prints
the exact native command without mutating cloud resources:

```bash
inferqos deploy --target aca --resource-group qos-production \
  --config inferqos.yaml --dry-run
```

Use `inferqos doctor --target <target>` after normal provider login to verify the local CLI,
authentication, DNS/TLS, coordinator, upstream, and capacity configuration.
