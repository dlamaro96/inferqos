#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail

binary=${1:?usage: install.sh BINARY CONFIG}
config=${2:?usage: install.sh BINARY CONFIG}
test -x "$binary" || { echo "InferQoS binary is not executable: $binary" >&2; exit 2; }
test -r "$config" || { echo "InferQoS configuration is not readable: $config" >&2; exit 2; }

if ! getent group inferqos >/dev/null; then groupadd --system inferqos; fi
if ! id inferqos >/dev/null 2>&1; then
  useradd --system --gid inferqos --home-dir /var/lib/inferqos --shell /usr/sbin/nologin inferqos
fi
install -d -o root -g inferqos -m 0750 /etc/inferqos
install -d -o inferqos -g inferqos -m 0750 /var/lib/inferqos /var/lib/inferqos/spool
install -o root -g root -m 0755 "$binary" /usr/local/bin/inferqos
install -o root -g inferqos -m 0640 "$config" /etc/inferqos/inferqos.yaml
install -o root -g root -m 0644 deploy/systemd/inferqos.service /etc/systemd/system/inferqos.service
systemctl daemon-reload
systemctl enable --now inferqos
for _ in $(seq 1 30); do
  if /usr/bin/curl -fsS http://127.0.0.1:9090/health/ready >/dev/null; then
    echo "InferQoS is ready. Proxy: http://127.0.0.1:8080  Admin: http://127.0.0.1:9090"
    exit 0
  fi
  sleep 1
done
systemctl --no-pager --full status inferqos >&2 || true
journalctl -u inferqos -n 50 --no-pager >&2 || true
echo "InferQoS did not become ready within 30 seconds" >&2
exit 1
