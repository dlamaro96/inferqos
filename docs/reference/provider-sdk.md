# Provider SDK and external protocol

In-process Rust adapters implement `inferqos_provider_sdk::ProviderAdapter` and run
`validate_estimate` plus the repository conformance suite. Adapters estimate, stream dispatch,
report health, map throttles/retry-after, and reconcile actual work without exposing vendor types to
the scheduler.

External adapters implement `protocol/provider/v1/provider.proto` over a local Unix socket by
default. Localhost TCP is acceptable; remote use requires mutually authenticated TLS. Version 1 is
experimental during InferQoS 0.x. Minor additive fields remain wire-compatible; breaking RPC or
semantic changes require a new protobuf package. Plugins are privileged code: pin artifacts,
restrict filesystem/network access, and treat compromise as equivalent to provider credential loss.

The runnable reference service is `cargo run -p inferqos-provider-protocol --bin example-adapter`.
See [the external protocol reference](external-provider-protocol.md) for transport controls and an
end-to-end configuration.
