# PlugLayer

PlugLayer is not required by InferQoS. It is one OCI-compatible deployment option used by the
project for public demonstrations and normal container validation.

Keep these workloads separate:

- the public static website and browser-only `/demo/`;
- an ephemeral runtime validation project containing InferQoS and the deterministic fake provider;
- any production InferQoS deployment connected to real capacity.

The checked-in `web/site/Dockerfile` builds the public site. The disposable
`deploy/pluglayer/Dockerfile.runtime-test` embeds the zero-key demo configuration for validation
only. It is not the production image and must not be pointed at a real provider.

Production deployments should use the signed release image, mount a validated configuration,
restrict the management listener to internal networks, configure bearer authentication if remote
access is required, and use Valkey when more than one replica admits against the same pool.

The public site sends no analytics. Its simulation runs entirely in the visitor's browser.
