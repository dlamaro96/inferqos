---
title: "ADR 0011: Security, identity, and extension hardening"
---

# ADR 0011: Security, identity, and extension hardening

- Status: Accepted
- Date: 2026-09-05

## Context

Production admission requires identity that cannot be forged, bounded storage, provider
credentials that do not become static secrets, and an extension boundary that preserves streaming.
Durable background delivery is useful but must not enter the synchronous hot path.

## Decision

Use rustls-verified direct mTLS fingerprints, HTTPS OIDC discovery/JWKS verification, and explicit
trusted-proxy CIDRs before entitlement resolution. Use ambient cloud SDK credential chains for AWS,
GCP, and Azure adapters. Spool only bounded request bodies to exclusive temporary files. Export
telemetry through operator-configured OTLP only. Implement durable jobs as an optional at-least-once
trait with NATS JetStream and Service Bus adapters. Implement third-party providers through the
versioned streaming gRPC protocol over Unix sockets, loopback, or explicit TLS/mTLS.

## Consequences

Trust-root, pool, listener, and coordinator changes need a rolling restart; ordinary policy and
mapping changes can hot reload. Remote adapters are privileged and need process/network isolation.
Cloud integration tests require explicitly supplied isolated credentials. The data plane continues
to have no mandatory broker, database, cloud SDK call, or telemetry destination.
