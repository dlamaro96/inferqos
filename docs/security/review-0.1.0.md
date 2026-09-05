---
title: "Security best-practices review — 0.1.0"
---

# Security best-practices review — 0.1.0

## Executive summary

The review covered the Go SDK, browser dashboard, proxy trust boundary, deployment manifests, and
secret/config handling. No critical finding remains open. One medium DOM-XSS risk and one high
admin-authentication gap were fixed before publication. Residual deployment controls are documented
below and must be verified at the operator’s edge.

## Resolved findings

### IQS-SEC-001 — unsafe dashboard HTML construction

- Severity: Medium
- Location: `crates/proxy/src/lib.rs`, dashboard script, line 725
- Evidence: the first implementation assigned API-derived values through `innerHTML`.
- Impact: a malicious configuration label returned by the admin API could have been interpreted as
  markup when an operator opened the internal dashboard.
- Fix: the dashboard now constructs nodes and assigns all values through `textContent`; a CSP,
  `nosniff`, frame denial, and no-referrer policy are added by `admin_security` (line 673).
- Mitigation: keep the admin listener internal even with application authentication.

### IQS-SEC-002 — configured admin bearer token was not enforced

- Severity: High
- Location: `crates/proxy/src/lib.rs`, `admin_security`, line 673
- Evidence: management routes originally had no middleware consuming `admin.bearer_token_env`.
- Impact: if an operator exposed the admin listener beyond the documented loopback/private boundary,
  queue, capacity, and decision metadata could be read without authentication.
- Fix: every non-health management/UI route now enforces the configured bearer value with a
  constant-time comparison and fails closed when its environment variable is absent.
- Mitigation: cloud ingress remains internal; apply network policy and do not use a public listener.

### IQS-SEC-003 — HA coordination was not on the admission path

- Severity: High
- Location: `crates/proxy/src/lib.rs`, admission and stream lease lifecycle, lines 300–480
- Evidence: the original proxy used only a per-process capacity ledger despite validating Valkey.
- Impact: multiple replicas could independently admit beyond one shared pool’s hard limit.
- Fix: all admissions now reserve atomically through the configured coordinator, track locally for
  telemetry, renew during streams, release idempotently, and rely on expiry after crashes.
- Mitigation: startup rejects expected replica counts above one with an in-memory coordinator unless
  the operator explicitly acknowledges isolated pools.

## Residual verification

- Rust is the network-facing implementation; the available language-specific skill has no Rust
  reference. The repository threat model and Rust tests cover bounds, spoofing, SSRF, leases, and
  malformed input, but an independent Rust audit is still appropriate before high-impact deployment.
- The Go code is a header-only client helper: it does not serve HTTP, execute commands, parse bodies,
  persist secrets, or trust forwarded headers. `go test ./...` passes.
- TLS termination, cloud firewall rules, Valkey ACL/TLS, OIDC/mTLS policy, and egress restrictions are
  environment controls and must be verified in the deployed topology.

## Completion hardening

- OIDC accepts only explicitly configured issuers and audiences, bounds discovery/JWKS responses,
  disables redirects, permits only RS/ES JWT algorithms, and refreshes an unknown signing key once.
- Direct mTLS uses a configured client CA; trusted-proxy identity headers are accepted only from
  configured CIDRs, and certificate SAN/fingerprint mappings remain policy controlled.
- Queued bodies use an owner-only spool directory, exclusive file creation, bounded bytes, and
  delete-on-drop handles. Payload logging and persistence remain disabled by default.
- The external provider protocol is local-socket/loopback-first, bounds messages, and supports TLS
  with mutual authentication for remote use.
- The final dependency graph passes `cargo audit` and `cargo deny check`; JWT verification uses the
  AWS-LC-backed implementation rather than the advisory-affected RSA implementation.
