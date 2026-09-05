---
title: "ADR 0006: Provider-neutral capacity interfaces"
---

# ADR 0006: Provider-neutral capacity interfaces

- Status: Accepted
- Date: 2026-09-05

Core scheduling consumes `WorkEstimate`, `WorkUnits`, health, and throttle feedback. HTTP/provider
authentication and vendor terminology remain in adapters. Third-party protocol evolution is
versioned independently from scheduler internals.
