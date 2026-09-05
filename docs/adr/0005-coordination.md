---
title: "ADR 0005: Lease-based coordination"
---

# ADR 0005: Lease-based coordination

- Status: Accepted
- Date: 2026-09-05

Single instances use an in-memory atomic reservation ledger. HA deployments use Valkey Lua
scripts to reserve capacity and create expiring leases atomically. Release and actual-work
correction are idempotent. Lease expiry recovers capacity after crashes. If coordination fails,
admission fails closed by default to protect contracted capacity; explicitly configured shadow
mode remains observational.
