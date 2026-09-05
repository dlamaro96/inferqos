---
title: "ADR 0003: Rust-first single process"
---

# ADR 0003: Rust-first single process

- Status: Accepted
- Date: 2026-09-05

The default deployment is one Rust binary containing proxy, scheduler, capacity ledger,
management API, simulator, and embedded dashboard. Tokio provides nonblocking I/O; Axum/Tower
provide HTTP plumbing. Single-replica operation has no external dependency. Multiple replicas
sharing a finite pool require the Valkey coordinator.
