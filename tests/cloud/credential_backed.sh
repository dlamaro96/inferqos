#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
set -euo pipefail

# Explicitly opt in because these checks use the operator's normal cloud identity.
case "${INFERQOS_CLOUD_TEST_TARGET:-}" in
  aws) aws sts get-caller-identity >/dev/null; cargo run --bin inferqos --features cloud-auth -- doctor --config "${INFERQOS_CLOUD_TEST_CONFIG}" --target ecs ;;
  gcp) gcloud auth print-access-token >/dev/null; cargo run --bin inferqos --features cloud-auth -- doctor --config "${INFERQOS_CLOUD_TEST_CONFIG}" --target cloud-run ;;
  azure) echo "Azure tests require an explicitly supplied non-production test subscription and are never auto-selected." >&2; az account show >/dev/null; cargo run --bin inferqos --features cloud-auth -- doctor --config "${INFERQOS_CLOUD_TEST_CONFIG}" --target aca ;;
  *) echo "Set INFERQOS_CLOUD_TEST_TARGET to aws, gcp, or azure and INFERQOS_CLOUD_TEST_CONFIG to an isolated credential-backed fixture." >&2; exit 2 ;;
esac
