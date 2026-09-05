#!/bin/sh
# Generate concurrent metadata-only demo traffic. Requires the Docker demo to be running.
set -eu
endpoint=${INFERQOS_DEMO_URL:-http://127.0.0.1:8080/v1/chat/completions}
send() { class=$1; token=$2; text=$3; curl -fsS "$endpoint" -H 'content-type: application/json' -H "authorization: Bearer $token" -H "x-inferqos-class: $class" -d "{\"model\":\"fake\",\"messages\":[{\"role\":\"user\",\"content\":\"$text\"}]}" >/dev/null || true; }
i=0; while [ "$i" -lt 20 ]; do send batch local-demo-background "large continuous batch request $i ............................................................" & i=$((i+1)); done
sleep 0.05
i=0; while [ "$i" -lt 8 ]; do send workflow local-demo-background "workflow $i" & i=$((i+1)); done
i=0; while [ "$i" -lt 8 ]; do send interactive local-demo-interactive "interactive $i" & i=$((i+1)); done
wait
echo "Demo load complete. Inspect http://127.0.0.1:9090/ui and /api/v1/decisions."
