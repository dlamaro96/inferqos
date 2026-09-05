#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail

binary=${INFERQOS_TEST_BINARY:-target/debug/inferqos}
upstream=${INFERQOS_UPSTREAM:-http://127.0.0.1:18080/}
test_root=$(mktemp -d "${TMPDIR:-/tmp}/inferqos-reload.XXXXXX")
config="$test_root/config.yaml"
spool="$test_root/spool"
log="$test_root/inferqos.log"
reload_token=${INFERQOS_RELOAD_TEST_VALUE:-reload-secret}
cp tests/fixtures/hot-reload.yaml "$config"

cleanup() {
  if [[ -n "${server_pid:-}" ]]; then kill -TERM "$server_pid" 2>/dev/null || true; wait "$server_pid" 2>/dev/null || true; fi
  find "$test_root" -depth -delete
}
trap cleanup EXIT INT TERM

INFERQOS_UPSTREAM="$upstream" INFERQOS_TEST_SPOOL="$spool" \
INFERQOS_RELOAD_TEST_KEY="$reload_token" "$binary" serve --config "$config" >"$log" 2>&1 &
server_pid=$!
for _ in $(seq 1 100); do
  curl -fsS http://127.0.0.1:9290/health/ready >/dev/null 2>&1 && break
  sleep 0.05
done
curl -fsS http://127.0.0.1:9290/health/ready >/dev/null

request() {
  curl -fsS -o /dev/null \
    -H "authorization: Bearer $reload_token" \
    -H 'x-inferqos-class: interactive' \
    -H 'content-type: application/json' \
    --data '{"model":"fake","messages":[{"role":"user","content":"this body is deliberately larger than eight bytes"}],"max_tokens":8}' \
    http://127.0.0.1:8280/v1/chat/completions
}

request
curl -fsS http://127.0.0.1:9290/api/v1/decisions | grep -q '"effective_class":"interactive"'
test "$(find "$spool" -type f | wc -l | tr -d ' ')" = 0
mode=$(stat -f '%Lp' "$spool" 2>/dev/null || stat -c '%a' "$spool")
test "$mode" = 700

sed -i.bak 's/allowed_classes: \[interactive, standard\]/allowed_classes: [standard]/' "$config"
for _ in $(seq 1 100); do
  grep -q 'validated configuration reloaded' "$log" && break
  sleep 0.05
done
request
curl -fsS http://127.0.0.1:9290/api/v1/decisions | tail -c 700 | grep -q '"effective_class":"standard"'

sed -i.bak 's/kind: InferQoSConfig/kind: BrokenConfig/' "$config"
sleep 0.4
request
curl -fsS http://127.0.0.1:9290/api/v1/decisions | tail -c 700 | grep -q '"effective_class":"standard"'
grep -q 'configuration reload rejected; retaining known-good configuration' "$log"

echo 'secure spooling, valid hot reload, and invalid-config rollback passed'
