$ErrorActionPreference = 'Stop'
$Repo = 'SulthanZahran1/rust_mutant'
$Version = if ($env:RUST_MUTANT_VERSION) { $env:RUST_MUTANT_VERSION } else { 'latest' }
$InstallDir = if ($env:RUST_MUTANT_INSTALL_DIR) { $env:RUST_MUTANT_INSTALL_DIR } else { Join-Path $HOME '.local\bin' }
$Target = 'x86_64-pc-windows-msvc'
$Archive = "rust-mutant-$Target.zip"
$Base = if ($Version -eq 'latest') { "https://github.com/$Repo/releases/latest/download" } else { "https://github.com/$Repo/releases/download/v$Version" }
$Temp = Join-Path ([System.IO.Path]::GetTempPath()) ("rust-mutant-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $Temp | Out-Null
try {
  Invoke-WebRequest "$Base/$Archive" -OutFile "$Temp\$Archive"
  Invoke-WebRequest "$Base/SHA256SUMS" -OutFile "$Temp\SHA256SUMS"
  $Expected = (Select-String -Path "$Temp\SHA256SUMS" -Pattern $Archive | Select-Object -First 1).Line.Split()[0]
  $Actual = (Get-FileHash "$Temp\$Archive" -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($Expected.ToLowerInvariant() -ne $Actual) { throw 'rust-mutant installer: checksum verification failed' }
  Expand-Archive "$Temp\$Archive" -DestinationPath "$Temp\unpack" -Force
  $Binary = Get-ChildItem "$Temp\unpack" -Filter 'rust-mutant.exe' -Recurse | Select-Object -First 1
  if (-not $Binary) { throw 'rust-mutant installer: binary missing from archive' }
  New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
  Copy-Item $Binary.FullName (Join-Path $InstallDir 'rust-mutant.exe') -Force
  & (Join-Path $InstallDir 'rust-mutant.exe') --version
  Write-Output "installed rust-mutant in $InstallDir"
} finally {
  Remove-Item $Temp -Recurse -Force -ErrorAction SilentlyContinue
}
