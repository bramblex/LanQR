function Copy-WithFriendlyLockError {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Source,
        [Parameter(Mandatory = $true)]
        [string]$Destination,
        [Parameter(Mandatory = $true)]
        [string]$Label
    )

    try {
        Copy-Item -LiteralPath $Source -Destination $Destination -Force
    }
    catch {
        throw "Failed to copy $Label to dist. The destination file may be in use: $Destination`nPlease close the running program and build again."
    }
}

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$distRoot = Join-Path $repoRoot "dist"
$packageRoot = Join-Path $distRoot "LanQR"
$releaseDir = Join-Path $repoRoot "target\release"
$binaryPath = Join-Path $releaseDir "LanQR.exe"

Write-Host "==> Building LanQR release binary"
Push-Location $repoRoot
try {
    cargo build --release
}
finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $binaryPath)) {
    throw "Build succeeded but LanQR.exe was not found at: $binaryPath"
}

Write-Host "==> Preparing dist directory"
New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null

$legacyQrcp = Join-Path $packageRoot "qrcp.exe"
if (Test-Path -LiteralPath $legacyQrcp) {
    Remove-Item -LiteralPath $legacyQrcp -Force
}

Write-Host "==> Copying files to dist\LanQR"
Copy-WithFriendlyLockError -Source $binaryPath -Destination (Join-Path $packageRoot "LanQR.exe") -Label "LanQR.exe"
Copy-WithFriendlyLockError -Source (Join-Path $repoRoot "README.md") -Destination (Join-Path $packageRoot "README.md") -Label "README.md"

Write-Host ""
Write-Host "Build completed successfully."
Write-Host "Output directory: $packageRoot"
