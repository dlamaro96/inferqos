# Deployment

`inferqos deploy --target docker|aca|kubernetes|ecs|cloud-run|systemd` validates the required CLI,
shows the command, runs it, and directs the operator to readiness. Cloud authentication remains the
operator’s normal `az`, `aws`, or `gcloud` login. Production deployments use private networking,
workload identity/task roles/service accounts, secret references, at least two warm replicas, and
Valkey for shared pools. Never expose the admin listener publicly.

For rolling updates, terminate only after readiness removal and allow the 30-second drain budget.
Validate new configuration before deployment. Roll back the image tag and prior Git-managed config;
ephemeral reservations recover through lease expiry.

