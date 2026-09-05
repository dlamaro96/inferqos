---
title: "Deployment"
---

# Deployment

`inferqos deploy --target docker|aca|kubernetes|ecs|cloud-run|systemd` validates the required CLI,
shows the command, runs it, and directs the operator to readiness. Cloud authentication remains the
operator’s normal `az`, `aws`, or `gcloud` login. Production deployments use private networking,
workload identity/task roles/service accounts, secret references, at least two warm replicas, and
Valkey for shared pools. Never expose the admin listener publicly.

Examples:

```bash
inferqos deploy --target docker --config inferqos.yaml
inferqos deploy --target kubernetes --config inferqos.yaml
inferqos deploy --target aca --resource-group qos-production --config inferqos.yaml
inferqos deploy --target ecs --region us-east-1 --vpc-id vpc-... \
  --subnets subnet-a,subnet-b --config-secret arn:aws:secretsmanager:... --config inferqos.yaml
inferqos deploy --target cloud-run --project my-project --region us-central1 \
  --config-secret inferqos-config --config inferqos.yaml
```

Use `--dry-run` to inspect the exact command without authentication or mutation. The ACA Bicep and
Terraform deployments use a user-assigned identity, internal ingress, warm replicas, workload
profiles, and a secret-backed configuration. ECS uses task roles, private Fargate networking,
Secrets Manager, deployment health percentages, and no public task IP. Cloud Run uses a service
account, Secret Manager, internal ingress, warm instances, and bounded concurrency. The systemd
wrapper installs a dedicated unprivileged account, protected unit, root-owned configuration, and
performs a readiness check.

Cloud credential-backed tests are deliberately opt-in so a developer machine can never select an
account by accident:

```bash
INFERQOS_CLOUD_TEST_TARGET=aws \
INFERQOS_CLOUD_TEST_CONFIG=tests/fixtures/aws.yaml \
tests/cloud/credential_backed.sh
```

Use `gcp` similarly. The `azure` case requires an explicitly isolated test subscription and is
never selected automatically. These tests consume the operator's ambient short-lived identity and
never accept long-lived credentials as arguments.

For rolling updates, terminate only after readiness removal and allow the 30-second drain budget.
Validate new configuration before deployment. Roll back the image tag and prior Git-managed config;
ephemeral reservations recover through lease expiry.

For the project website and an isolated OCI validation example, see the
[PlugLayer deployment boundary](pluglayer.html). PlugLayer is optional and is not a runtime
dependency of InferQoS.
