param(
    [string]$SourceDir = (Join-Path $env:TEMP 'smalltalkai-sherpa-webgpu-1.13.8')
)
$ErrorActionPreference = 'Stop'

# Build only the Sherpa C API. The installed OCR/TTS runtime supplies ONNX Runtime.
$revision = '11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf'
if (-not (Test-Path -LiteralPath (Join-Path $SourceDir '.git'))) {
    git clone --quiet --depth 1 --branch v1.13.8 https://github.com/k2-fsa/sherpa-onnx.git $SourceDir
    if ($LASTEXITCODE -ne 0) { throw 'Could not fetch Sherpa-ONNX' }
}
$head = git -C $SourceDir rev-parse HEAD
if ($LASTEXITCODE -ne 0 -or $head -ne $revision) { throw 'Unexpected Sherpa-ONNX source revision' }

$patch = Join-Path $PSScriptRoot 'sherpa-webgpu.patch'
git -C $SourceDir apply --reverse --check $patch 2>$null
if ($LASTEXITCODE -ne 0) {
    git -C $SourceDir apply $patch
    if ($LASTEXITCODE -ne 0) { throw 'Could not apply the shared Sherpa WebGPU patch' }
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
$vcvars = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'VC/Auxiliary/Build/vcvars64.bat' | Select-Object -First 1
if (-not $vcvars) { throw 'Visual Studio C++ Build Tools are required for the WebGPU Sherpa DLL' }
$build = Join-Path $SourceDir 'build'
$driver = Join-Path $SourceDir 'build-webgpu.cmd'
@"
@echo off
call "$vcvars" >nul || exit /b 1
cmake -S "$SourceDir" -B "$build" -G Ninja -DCMAKE_VS_PLATFORM_NAME=x64 -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON -DSHERPA_ONNX_ENABLE_BINARY=OFF -DSHERPA_ONNX_ENABLE_PORTAUDIO=OFF -DSHERPA_ONNX_ENABLE_WEBSOCKET=OFF || exit /b 1
cmake --build "$build" --parallel 12 --target sherpa-onnx-c-api
"@ | Set-Content -LiteralPath $driver -Encoding ascii
cmd /d /c $driver | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'Sherpa WebGPU build failed' }
$dll = Join-Path $build 'bin/sherpa-onnx-c-api.dll'
if (-not (Test-Path -LiteralPath $dll -PathType Leaf)) { throw 'Sherpa WebGPU DLL is missing' }
$dll
