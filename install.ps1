# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = "Stop"
$repo = "dlamaro96/inferqos"
$version = if ($env:INFERQOS_VERSION) { $env:INFERQOS_VERSION } else { (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name }
$asset = "inferqos-$version-windows-amd64.tar.gz"
$temp = Join-Path ([IO.Path]::GetTempPath()) ("inferqos-" + [Guid]::NewGuid())
$dest = if ($env:INFERQOS_INSTALL_DIR) { $env:INFERQOS_INSTALL_DIR } else { Join-Path $HOME ".local\bin" }
if (-not (Get-Command cosign -ErrorAction SilentlyContinue)) { throw "cosign is required to verify the release signature and build identity; see https://docs.sigstore.dev/cosign/system_config/installation/" }
New-Item -ItemType Directory -Path $temp,$dest -Force | Out-Null
try {
  $base = "https://github.com/$repo/releases/download/$version"
  Invoke-WebRequest "$base/$asset" -OutFile (Join-Path $temp $asset)
  Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $temp "SHA256SUMS")
  Invoke-WebRequest "$base/SHA256SUMS.bundle" -OutFile (Join-Path $temp "SHA256SUMS.bundle")
  & cosign verify-blob (Join-Path $temp "SHA256SUMS") --bundle (Join-Path $temp "SHA256SUMS.bundle") --certificate-oidc-issuer "https://token.actions.githubusercontent.com" --certificate-identity-regexp "^https://github.com/dlamaro96/inferqos/.github/workflows/release.yml@refs/tags/v" | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "Sigstore signature or release workflow identity verification failed" }
  $expected = ((Get-Content (Join-Path $temp "SHA256SUMS")) | Where-Object { $_ -match [regex]::Escape($asset) }).Split()[0]
  $actual = (Get-FileHash (Join-Path $temp $asset) -Algorithm SHA256).Hash.ToLower()
  if ($expected -ne $actual) { throw "Checksum verification failed" }
  if (Get-Command gh -ErrorAction SilentlyContinue) {
    & gh attestation verify (Join-Path $temp $asset) --repo $repo | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "GitHub build-provenance verification failed" }
  } else { Write-Warning "Sigstore build identity verified. Install authenticated GitHub CLI to additionally verify the GitHub artifact attestation." }
  tar -xzf (Join-Path $temp $asset) -C $temp
  Copy-Item (Join-Path $temp "inferqos.exe") (Join-Path $dest "inferqos.exe") -Force
  Write-Host "Installed verified $version to $dest\inferqos.exe"
} finally { Remove-Item $temp -Recurse -Force -ErrorAction SilentlyContinue }
