$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    $cargoToml = Get-Content -Raw (Join-Path $repoRoot "Cargo.toml")
    if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
        $version = $Matches[1]
    } else {
        throw "Could not determine workspace version from Cargo.toml"
    }

    Write-Host "==> Building StudyNodes v$version release binary..." -ForegroundColor Cyan
    cargo build --release -p studynodes-app
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "==> Building self-contained Windows installer (studynodes-setup)..." -ForegroundColor Cyan
    cargo build --release -p studynodes-installer --features embed-app
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $srcSetup = Join-Path $repoRoot "target\release\studynodes-setup.exe"
    $dstSetup = Join-Path $repoRoot "target\release\StudyNodes-Setup-v$version-windows-x86_64.exe"
    Copy-Item -Force $srcSetup $dstSetup

    $sizeMb = [Math]::Round((Get-Item $dstSetup).Length / 1MB, 2)
    Write-Host "==> Installer ready: $dstSetup ($sizeMb MB)" -ForegroundColor Green
} finally {
    Pop-Location
}
