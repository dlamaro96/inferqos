#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail

requests="${INFERQOS_LOAD_REQUESTS:-200}"
concurrency="${INFERQOS_LOAD_CONCURRENCY:-20}"
results="$(mktemp "${TMPDIR:-/tmp}/inferqos-load.XXXXXX")"
trap 'rm -f "$results"' EXIT

seq "$requests" | xargs -P "$concurrency" -I{} curl -sS -o /dev/null -w '%{http_code}\n' \
  -H 'content-type: application/json' \
  -H 'x-inferqos-class: standard' \
  --data '{"model":"fake","messages":[{"role":"user","content":"bounded load test"}],"max_tokens":8}' \
  http://127.0.0.1:8080/v1/chat/completions >"$results"

unexpected=$(grep -Evc '^(200|429)$' "$results" || true)
admitted=$(grep -c '^200$' "$results" || true)
rejected=$(grep -c '^429$' "$results" || true)
test "$unexpected" = 0
test "$admitted" -gt 0
test $((admitted + rejected)) = "$requests"

curl -fsS http://127.0.0.1:9090/metrics | grep -q '^inferqos_requests_total '
echo "bounded load passed: admitted=$admitted rejected=$rejected total=$requests"
