#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail

requests="${INFERQOS_LOAD_REQUESTS:-200}"
concurrency="${INFERQOS_LOAD_CONCURRENCY:-20}"
seq "$requests" | xargs -P "$concurrency" -I{} curl -fsS -o /dev/null \
  -H 'content-type: application/json' \
  -H 'x-inferqos-class: standard' \
  --data '{"model":"fake","messages":[{"role":"user","content":"bounded load test"}],"max_tokens":8}' \
  http://127.0.0.1:8080/v1/chat/completions
curl -fsS http://127.0.0.1:9090/metrics | grep -q '^inferqos_requests_total '
