set shell := ["bash", "-euo", "pipefail", "-c"]

dev:
    cargo run --bin inferqos -- serve --config examples/minimal/demo.yaml

test:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    (cd sdk/typescript && npm test)
    (cd sdk/go && go test ./...)
    node --check web/site/site.js
    node --check web/site/demo/demo.js

fmt:
    cargo fmt --all

lint:
    cargo clippy --workspace --all-targets -- -D warnings

demo:
    docker compose -f deploy/docker/compose.yaml up --build

benchmark:
    cargo run --release --bin inferqos -- benchmark --decisions 100000

docs:
    test -f docs/index.md

site:
    docker build -f web/site/Dockerfile -t inferqos-site:dev .
    docker run --rm -p 3000:8080 inferqos-site:dev

site-test:
    node --check web/site/site.js
    node --check web/site/demo/demo.js
    docker build -f web/site/Dockerfile -t inferqos-site:test .
    container=inferqos-site-test; trap 'docker rm -f "$container" >/dev/null 2>&1 || true' EXIT; docker run --rm -d --name "$container" --read-only -p 3000:8080 inferqos-site:test >/dev/null; for _ in $(seq 1 60); do curl -fsS http://127.0.0.1:3000/healthz >/dev/null && break; sleep 0.2; done; tests/integration/site.sh

security:
    cargo audit
    cargo deny check

schema:
    cargo run -p inferqos-config --example schema -- config.schema.json

release-check: test lint benchmark docs site-test
    git diff --exit-code
