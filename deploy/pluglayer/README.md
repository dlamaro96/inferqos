# PlugLayer validation deployment

The production website and the disposable runtime validation stack are intentionally separate
PlugLayer projects.

- The permanent public website and browser-only simulator are built from a separate
  private-source repository.
- `Dockerfile.runtime-test` embeds only the zero-key demo configuration for disposable live tests.
- `deploy/docker/Dockerfile.fake` builds the deterministic finite-capacity provider used by tests.

Verified releases publish the fake provider as `ghcr.io/dlamaro96/inferqos:fake-vX.Y.Z` and include
the demo configuration inside the main image at `/usr/share/inferqos/examples/demo.yaml`. This keeps
zero-key validation reproducible without making the fake provider part of a production request path.

The runtime test image is not a production distribution. Production installations should mount a
validated configuration and protect the management listener with a bearer token or internal-only
ingress.
