# InferQoS public site

The public site is a dependency-free static build embedded in a small, unprivileged Rust server.
Its release image uses the same minimal distroless runtime as the data plane and has no shell or package manager.
The `/demo/` route runs a deterministic educational scheduler simulation entirely in the browser.
It sends no analytics or workload data.

```bash
docker build -f web/site/Dockerfile -t inferqos-site .
docker run --rm -p 3000:8080 inferqos-site
```

Validate JavaScript and production headers with `just site-test`.
