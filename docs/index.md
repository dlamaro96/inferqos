---
title: "Index"
---

---
title: Documentation
description: Deploy, operate, and extend the InferQoS control plane.
---

<section class="docs-hero">
  <small>ENGINEERING DOCUMENTATION</small>
  <h1>Make finite inference capacity behave like infrastructure.</h1>
  <p>Deploy the Rust data plane, define service classes and fair shares, evaluate in shadow mode, and understand every admission decision.</p>
  <nav><a href="https://github.com/dlamaro96/inferqos#run-the-live-demo">Run the five-minute demo</a><a href="https://web.inferqos-website.daniamaro96.apps.pluglayer.io/demo/">Open the traffic lab</a></nav>
</section>

<div class="docs-note"><i></i><p>No paid provider key is required for the local demo. The default configuration logs no prompts or completions and sends no project analytics.</p></div>

## Choose a path

<div class="doc-grid">
  <a class="doc-card" href="architecture/overview.html"><small>UNDERSTAND</small><h2>Architecture</h2><p>Request path, trust boundaries, high availability, and the provider-neutral capacity model.</p></a>
  <a class="doc-card" href="concepts/scheduling.html"><small>REASON</small><h2>Scheduling</h2><p>Hierarchical fairness, work estimates, deadlines, aging, and non-preemption.</p></a>
  <a class="doc-card" href="deployment/"><small>SHIP</small><h2>Deployment</h2><p>One binary from Docker to Kubernetes, ACA, ECS, Cloud Run, and systemd.</p></a>
  <a class="doc-card" href="operations/runbook.html"><small>OPERATE</small><h2>Runbook</h2><p>Capacity pressure, coordinator failures, SLOs, alerts, and graceful degradation.</p></a>
</div>

## The shortest useful path

```bash
git clone https://github.com/dlamaro96/inferqos.git
cd inferqos
just demo
```

The demo starts the fake finite-capacity provider, InferQoS, the operational dashboard, and a workload that proves interactive protection without starving batch work.

## Production entry points

- [Configure a real provider](providers.html)
- [Set identity and service-class entitlements](security/identity.html)
- [Deploy into your environment](deployment/)
- [Export OTLP signals and Prometheus metrics](operations/telemetry.html)
- [Run shadow mode before enforcing policy](concepts/scheduling.html)
- [Build a provider adapter](reference/provider-sdk.html)
---\n+title: \"Index\"\n+---\n+\n ---
