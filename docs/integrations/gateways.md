# Existing gateway integrations

## APIM

Use `Client → APIM → InferQoS → Azure provisioned deployment`. APIM retains authentication,
subscription, governance, and coarse rate limits. Strip all inbound `X-InferQoS-*` headers, then set
tenant/application from verified APIM identity; preserve `traceparent`. Use private endpoints and
allow only the APIM subnet. InferQoS handles capacity-aware admission, not APIM replacement.

## Kong and Envoy

Route OpenAI-compatible paths to InferQoS, preserve streaming/timeouts and tracing, remove untrusted
QoS identity headers, and add trusted identity metadata after gateway authentication. Keep retries
disabled after response bytes begin. InferQoS forwards to a static configured pool; it is not a
semantic router.

