# ADR 0009: One binary, OCI everywhere

- Status: Accepted
- Date: 2026-09-05

One binary serves data/admin planes and embedded UI. OCI is primary but not mandatory. Kubernetes
has a Helm chart without an operator; VM deployments use systemd; cloud templates use native identity.

