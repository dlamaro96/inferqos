#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail

# Run against a locally started InferQoS instance, then terminate the fake
# upstream. Fail-closed behavior must be visible and bounded.
provider_pid="${INFERQOS_FAKE_PROVIDER_PID:?set the fake-provider process ID}"

request='{"model":"fake","messages":[{"role":"user","content":"chaos"}],"max_tokens":8}'
code=$(curl -sS -D /tmp/inferqos-throttle-headers -o /tmp/inferqos-throttle-body -w '%{http_code}' \
  -H 'content-type: application/json' -H 'x-fake-mode: throttle' --data "$request" \
  http://127.0.0.1:8080/v1/chat/completions)
test "$code" = 429
grep -qi '^retry-after: 1' /tmp/inferqos-throttle-headers

code=$(curl -sS -o /tmp/inferqos-malformed -w '%{http_code}' \
  -H 'content-type: application/json' -H 'x-fake-mode: malformed' --data "$request" \
  http://127.0.0.1:8080/v1/chat/completions)
test "$code" = 200
grep -q '{this-is-not-json' /tmp/inferqos-malformed
curl -fsS http://127.0.0.1:9090/health/ready >/dev/null

# Cancellation while waiting for upstream headers and during an upstream body
# must release reservations rather than waiting for lease expiry.
curl --max-time 0.05 -sS -o /dev/null -H 'content-type: application/json' \
  -H 'x-fake-mode: slow' --data "$request" \
  http://127.0.0.1:8080/v1/chat/completions || true
curl --max-time 0.05 -sS -o /dev/null -H 'content-type: application/json' \
  -H 'x-fake-mode: slow' --data '{"model":"fake","messages":[],"stream":true,"max_tokens":8}' \
  http://127.0.0.1:8080/v1/chat/completions || true
for _ in $(seq 1 100); do
  status_json=$(curl -fsS http://127.0.0.1:9090/api/v1/status)
  capacity_json=$(curl -fsS http://127.0.0.1:9090/api/v1/capacity)
  if echo "$status_json" | grep -q '"active":0' && echo "$capacity_json" | grep -q '"reserved_units":0.0'; then break; fi
  sleep 0.05
done
echo "$status_json" | grep -q '"active":0'
echo "$capacity_json" | grep -q '"reserved_units":0.0'

curl -sS -o /dev/null -H 'content-type: application/json' -H 'x-fake-mode: disconnect' \
  --data "$request" http://127.0.0.1:8080/v1/chat/completions || true
curl -fsS http://127.0.0.1:9090/health/ready >/dev/null

kill -TERM "$provider_pid"
for _ in $(seq 1 50); do
  code=$(curl -sS -o /tmp/inferqos-chaos-response -w '%{http_code}' \
    -H 'content-type: application/json' --data '{"model":"fake","messages":[]}' \
    http://127.0.0.1:8080/v1/chat/completions || true)
  if [ "$code" = 502 ] || [ "$code" = 503 ]; then exit 0; fi
  sleep 0.1
done
echo "InferQoS did not surface upstream failure within five seconds" >&2
exit 1
