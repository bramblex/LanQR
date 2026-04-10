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

function Get-CargoVersion {
    param(
        [Parameter(Mandatory = $true)]
        [string]$CargoTomlPath
    )

    $versionLine = Select-String -Path $CargoTomlPath -Pattern '^\s*version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if ($null -eq $versionLine) {
        throw "Failed to read version from Cargo.toml: $CargoTomlPath"
    }

    return $versionLine.Matches[0].Groups[1].Value
}

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$distRoot = Join-Path $repoRoot "dist"
$packageRoot = Join-Path $distRoot "LanQR"
$releaseDir = Join-Path $repoRoot "target\release"
$binaryPath = Join-Path $releaseDir "LanQR.exe"
$iconPath = Join-Path $repoRoot "target\generated\LanQR.ico"
$cargoTomlPath = Join-Path $repoRoot "Cargo.toml"

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

if (-not (Test-Path -LiteralPath $iconPath)) {
    throw "Build succeeded but LanQR.ico was not found at: $iconPath"
}

$gitHash = (git rev-parse --short HEAD).Trim()
if ([string]::IsNullOrWhiteSpace($gitHash)) {
    throw "Failed to resolve git short hash"
}

$exactTag = ((git describe --tags --exact-match HEAD 2>$null) | Out-String).Trim()
$releaseVersion = if (-not [string]::IsNullOrWhiteSpace($exactTag)) {
    $exactTag
}
else {
    "v$(Get-CargoVersion -CargoTomlPath $cargoTomlPath)"
}

Write-Host "==> Preparing dist directory"
New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null

$legacyQrcp = Join-Path $packageRoot "qrcp.exe"
if (Test-Path -LiteralPath $legacyQrcp) {
    Remove-Item -LiteralPath $legacyQrcp -Force
}
$packageIcon = Join-Path $packageRoot "LanQR.ico"

Write-Host "==> Copying files to dist\LanQR"
Copy-WithFriendlyLockError -Source $binaryPath -Destination (Join-Path $packageRoot "LanQR.exe") -Label "LanQR.exe"
Copy-WithFriendlyLockError -Source $iconPath -Destination $packageIcon -Label "LanQR.ico"
Copy-WithFriendlyLockError -Source (Join-Path $repoRoot "README.md") -Destination (Join-Path $packageRoot "README.md") -Label "README.md"

$zipName = "LanQR-$releaseVersion-$gitHash.zip"
$zipPath = Join-Path $distRoot $zipName
if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
}

Write-Host "==> Creating release zip"
Compress-Archive -LiteralPath $packageRoot -DestinationPath $zipPath -CompressionLevel Optimal

Write-Host ""
Write-Host "Build completed successfully."
Write-Host "Output directory: $packageRoot"
Write-Host "Zip archive: $zipPath"
