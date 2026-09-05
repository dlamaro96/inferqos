---
title: "External provider adapter protocol"
---

# External provider adapter protocol

`protocol/provider/v1/provider.proto` is executable, not an interface sketch. Run the included
adapter and a second local InferQoS instance:

```bash
INFERQOS_ADAPTER_LISTEN=127.0.0.1:50051 \
  cargo run -p inferqos-provider-protocol --bin example-adapter
cargo run --bin inferqos -- serve --config examples/external-provider/demo.yaml
```

Version 1 defines unary `Estimate` and `Health` RPCs plus server-streaming `Dispatch`. Response
headers/status are sent with the first chunk; body chunks remain bounded by HTTP/gRPC backpressure;
final usage can arrive on any chunk for post-stream reconciliation. Authorization, cookies, and
provider API keys are never forwarded to an external adapter as ordinary metadata.

Unix-domain sockets are preferred on one host. Plain TCP is rejected unless it resolves to
loopback. Remote TCP configuration requires TLS with an explicit CA and domain; client certificate
and key enable mTLS and must be configured together. Both encoding and decoding are capped at
16 MiB. Treat an adapter as privileged provider-facing code and isolate its filesystem and egress.

The protobuf package is `inferqos.provider.v1`. During InferQoS 0.x, additive fields preserve wire
compatibility; breaking RPC or semantic changes require another protobuf package version.
