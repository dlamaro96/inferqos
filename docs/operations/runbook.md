---
title: "Operations and SLO guide"
---

# Operations and SLO guide

Recommended internal objectives (not an OSS SLA): 99.9% data-plane availability, scheduler p95
below 2 ms, proxy incremental p95 below 10 ms, healthy coordinator for every HA admission, and
per-class deadline attainment matching business policy.

Alert on `up{job="inferqos"} == 0`, coordinator failures, queue depth above 80% of its bound,
interactive p95 near its max queue, deadline violations, provider 429 spikes, estimate-error EWMA
above 0.5, sustained utilization above 90% or below 20%, abnormal rejection, and old outstanding
leases. During coordinator failure, stop autoscaling demand and restore Valkey; enforcement fails
closed to protect capacity. Configuration belongs in Git; coordinator state is ephemeral and needs
no backup. Preserve optional replay reports according to local data policy.
