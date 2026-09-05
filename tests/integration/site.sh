#!/usr/bin/env bash
set -euo pipefail

site_url="${INFERQOS_SITE_URL:-http://127.0.0.1:3000}"

home_headers="$(mktemp)"
demo_headers="$(mktemp)"
trap 'rm -f "$home_headers" "$demo_headers"' EXIT

curl -fsS -D "$home_headers" -o /tmp/inferqos-site-home.html "$site_url/"
curl -fsS -D "$demo_headers" -o /tmp/inferqos-site-demo.html "$site_url/demo/"
grep -q "Finite AI capacity" /tmp/inferqos-site-home.html
grep -q "See what QoS changes" /tmp/inferqos-site-demo.html
grep -qi '^content-security-policy:' "$home_headers"
grep -qi '^x-content-type-options: nosniff' "$home_headers"
grep -qi '^x-frame-options: DENY' "$home_headers"
curl -fsS "$site_url/site.js" | grep -q "inferqos-site-theme"
curl -fsS "$site_url/demo/demo.js" | grep -q "function simulate"
curl -fsS "$site_url/assets/capacity-control.webp" -o /dev/null
test "$(curl -fsS "$site_url/healthz")" = "ok"
test "$(curl -sS -o /dev/null -w '%{http_code}' "$site_url/not-a-route")" = "404"
rm -f /tmp/inferqos-site-home.html /tmp/inferqos-site-demo.html
echo "site smoke test passed: $site_url"
