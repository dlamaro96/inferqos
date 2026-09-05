#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
set -eu
repo=dlamaro96/inferqos
version=${INFERQOS_VERSION:-latest}
prefix=${INFERQOS_INSTALL_DIR:-"$HOME/.local/bin"}
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) platform=linux-amd64;; Linux-aarch64|Linux-arm64) platform=linux-arm64;;
  Darwin-arm64) platform=macos-arm64;; Darwin-x86_64) platform=macos-amd64;;
  *) echo "Unsupported OS/architecture: $(uname -s)/$(uname -m)" >&2; exit 2;;
esac
command -v curl >/dev/null || { echo "curl is required" >&2; exit 2; }
command -v shasum >/dev/null || command -v sha256sum >/dev/null || { echo "shasum or sha256sum is required" >&2; exit 2; }
command -v cosign >/dev/null || { echo "cosign is required to verify the release signature and build identity; install it from https://docs.sigstore.dev/cosign/system_config/installation/" >&2; exit 2; }
tmp=$(mktemp -d "${TMPDIR:-/tmp}/inferqos.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
if [ "$version" = latest ]; then version=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1); fi
[ -n "$version" ] || { echo "Could not determine release version" >&2; exit 1; }
asset="inferqos-${version}-${platform}.tar.gz"; base="https://github.com/$repo/releases/download/$version"
curl -fL "$base/$asset" -o "$tmp/$asset"; curl -fL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"; curl -fL "$base/SHA256SUMS.bundle" -o "$tmp/SHA256SUMS.bundle"
cosign verify-blob "$tmp/SHA256SUMS" \
  --bundle "$tmp/SHA256SUMS.bundle" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  --certificate-identity-regexp "^https://github.com/dlamaro96/inferqos/.github/workflows/release.yml@refs/tags/v" >/dev/null \
  || { echo "Sigstore signature or release workflow identity verification failed" >&2; exit 1; }
(cd "$tmp" && if command -v sha256sum >/dev/null; then sha256sum -c SHA256SUMS --ignore-missing; else expected=$(awk -v a="$asset" '$2==a{print $1}' SHA256SUMS); actual=$(shasum -a 256 "$asset"|awk '{print $1}'); [ "$expected" = "$actual" ]; fi) || { echo "Checksum verification failed" >&2; exit 1; }
if command -v gh >/dev/null; then gh attestation verify "$tmp/$asset" --repo "$repo" >/dev/null || { echo "GitHub build-provenance verification failed" >&2; exit 1; }; else echo "Note: Sigstore build identity verified. Install authenticated GitHub CLI to additionally verify the GitHub artifact attestation." >&2; fi
mkdir -p "$prefix"; tar -xzf "$tmp/$asset" -C "$tmp"; install -m 0755 "$tmp/inferqos" "$prefix/inferqos"
echo "Installed verified $version to $prefix/inferqos"; case ":$PATH:" in *":$prefix:"*) :;; *) echo "Add $prefix to PATH";; esac
