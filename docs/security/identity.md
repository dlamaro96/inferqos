# Identity and entitlement boundary

InferQoS calculates the effective service class only after identity resolution. A raw
`X-InferQoS-Class` request is a request, never an entitlement.

Resolution precedence is direct verified mTLS certificate fingerprint, mTLS SAN asserted by a
trusted proxy, complete trusted identity headers, OIDC bearer token, mapped API key, then an
untrusted anonymous identity. Direct mTLS requires `server.tls` with a server certificate/key and a
client CA; rustls rejects an untrusted client certificate before its SHA-256 fingerprint can be
mapped. API-key comparison is constant time.

OIDC uses discovery or an explicit JWKS URL over HTTPS, disables redirects, limits JWKS documents
to 1 MiB, refreshes once for an unknown key ID, accepts only configured asymmetric RS/ES algorithms,
and validates signature, issuer, audience, expiration, not-before, and the configured principal,
tenant, and application claims. `required: true` turns missing or invalid authentication into 401.

Forwarded identity and client-certificate SAN headers are ignored unless the immediate TCP peer is
inside `identity.trusted_proxy_cidrs`. Configure those CIDRs as narrowly as possible, have the
gateway remove client-supplied copies, and prevent clients from reaching InferQoS around the
gateway. Hot reload may change mappings and trusted proxy networks; OIDC issuer/key-source and TLS
trust changes require a rolling restart.

Example:

```yaml
identity:
  oidc:
    issuer: https://issuer.example/
    audience: inferqos
    principal_claim: sub
    tenant_claim: tenant
    application_claim: azp
    required: true
  trusted_proxy_cidrs: [10.20.0.0/24]
  mtls_certificate_sha256_mappings:
    0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef:
      principal: build-agent
      tenant: engineering
      application: batch-evals
```
