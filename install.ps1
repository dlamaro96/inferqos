# SPDX-License-Identifier: Apache-2.0
$ErrorActionPreference = "Stop"
$repo = "dlamaro96/inferqos"
$version = if ($env:INFERQOS_VERSION) { $env:INFERQOS_VERSION } else { (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name }
$asset = "inferqos-$version-windows-amd64.tar.gz"
$temp = Join-Path ([IO.Path]::GetTempPath()) ("inferqos-" + [Guid]::NewGuid())
$dest = if ($env:INFERQOS_INSTALL_DIR) { $env:INFERQOS_INSTALL_DIR } else { Join-Path $HOME ".local\bin" }
New-Item -ItemType Directory -Path $temp,$dest -Force | Out-Null
try {
  $base = "https://github.com/$repo/releases/download/$version"
  Invoke-WebRequest "$base/$asset" -OutFile (Join-Path $temp $asset)
  Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $temp "SHA256SUMS")
  $expected = ((Get-Content (Join-Path $temp "SHA256SUMS")) | Where-Object { $_ -match [regex]::Escape($asset) }).Split()[0]
  $actual = (Get-FileHash (Join-Path $temp $asset) -Algorithm SHA256).Hash.ToLower()
  if ($expected -ne $actual) { throw "Checksum verification failed" }
  tar -xzf (Join-Path $temp $asset) -C $temp
  Copy-Item (Join-Path $temp "inferqos.exe") (Join-Path $dest "inferqos.exe") -Force
  Write-Host "Installed verified $version to $dest\inferqos.exe"
} finally { Remove-Item $temp -Recurse -Force -ErrorAction SilentlyContinue }

