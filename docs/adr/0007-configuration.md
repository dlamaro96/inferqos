# ADR 0007: Strict configuration as code

- Status: Accepted
- Date: 2026-09-05

YAML uses `inferqos.io/v1alpha1`, rejects unknown fields, expands named environment variables, and
performs semantic validation before runtime replacement. Failed reloads never replace known-good state.

