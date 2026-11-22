param(
    [switch]$SkipTests,
    [string]$CargoTargetDir
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Cargo was not found. Install the stable Rust toolchain first."
}
if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    throw "npm was not found. Install Node.js to build the Settings frontend."
}
Push-Location (Join-Path $Root 'web/settings')
try {
    npm ci
    if ($LASTEXITCODE -ne 0) { throw "npm ci failed with exit code $LASTEXITCODE" }
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "Settings frontend build failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}
python (Join-Path $Root 'scripts/verify-settings-frontend.py')
if ($LASTEXITCODE -ne 0) { throw "Settings frontend verification failed" }
$ResolvedTargetDir = if ($CargoTargetDir) {
    [System.IO.Path]::GetFullPath($CargoTargetDir)
} elseif ($env:CARGO_TARGET_DIR) {
    [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
} else {
    Join-Path $Root 'target'
}
$env:CARGO_TARGET_DIR = $ResolvedTargetDir
if (-not $SkipTests) {
    cargo test --workspace --all-targets
}
cargo build --release -p smalltalkai -p smalltalkai-settings-window --bin smalltalkai-capture --bin smalltalkai-infer --bin smalltalkai-ctl --bin smalltalkai-settings --bin smalltalkai-settings-window --bin smalltalkai-reader-hud
if ($LASTEXITCODE -ne 0) { throw "Core binary build failed" }
cargo build --release -p smalltalkai-reader -p smalltalkai-readerctl -p smalltalkai-tts -p smalltalkai-ocr
if ($LASTEXITCODE -ne 0) { throw "Reader binary build failed" }
$Expected = @("smalltalkai-capture.exe", "smalltalkai-infer.exe", "smalltalkai-ctl.exe", "smalltalkai-settings.exe", "smalltalkai-settings-window.exe", "smalltalkai-reader.exe", "smalltalkai-readerctl.exe", "smalltalkai-tts.exe", "smalltalkai-ocr.exe", "smalltalkai-reader-hud.exe")
foreach ($Name in $Expected) {
    $Path = Join-Path $ResolvedTargetDir "release\$Name"
    if (-not (Test-Path -LiteralPath $Path)) { throw "Expected binary was not built: $Path" }
    Write-Host "Built: $Path"
}
