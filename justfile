set shell := ["bash", "-euo", "pipefail", "-c"]

dev:
    cargo run --bin inferqos -- serve --config examples/minimal/demo.yaml

test:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    (cd sdk/typescript && npm test)
    (cd sdk/go && go test ./...)

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

security:
    cargo audit
    cargo deny check

schema:
    cargo run -p inferqos-config --example schema -- config.schema.json

release-check: test lint benchmark docs
    git diff --exit-code

