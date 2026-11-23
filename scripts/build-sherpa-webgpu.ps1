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

$session = Join-Path $SourceDir 'sherpa-onnx/csrc/session.cc'
$upstream = (git -C $SourceDir show "${revision}:sherpa-onnx/csrc/session.cc") -join "`n"
if ($LASTEXITCODE -ne 0) { throw 'Could not read the pinned Sherpa session source' }
$marker = '  Provider p = StringToProvider(new_provider_str);'
if (-not $upstream.Contains($marker)) { throw 'Sherpa provider entry point changed' }
$webgpu = @'
  if (new_provider_str == "webgpu") {
    Ort::SessionOptions options;
    options.SetIntraOpNumThreads(num_threads);
    options.SetInterOpNumThreads(num_threads);
    if (std::getenv("SMALLTALKAI_SHERPA_WEBGPU_TRACE")) {
      options.SetLogSeverityLevel(0);
    }
    const char *index = std::getenv("SMALLTALKAI_WEBGPU_ADAPTER_INDEX");
    const char *keys[] = {"adapterIndex"};
    const char *values[] = {index};
    Ort::ThrowOnError(Ort::GetApi().SessionOptionsAppendExecutionProvider(
        options, "WebGPU", keys, values, index ? 1 : 0));
    return options;
  }

  Provider p = StringToProvider(new_provider_str);
'@
$source = $upstream.Replace($marker, $webgpu).Replace('#include <algorithm>', "#include <algorithm>`n#include <cstdlib>") + "`n"
if ([IO.File]::ReadAllText($session) -cne $source) { [IO.File]::WriteAllText($session, $source) }

# Report adapter and driver failures through the C ABI instead of unwinding into Rust.
$apiRelative = 'sherpa-onnx/c-api/c-api.cc'
$api = Join-Path $SourceDir $apiRelative
$upstream = (git -C $SourceDir show "${revision}:$apiRelative") -join "`n"
if ($LASTEXITCODE -ne 0) { throw 'Could not read the pinned Sherpa C API source' }
$marker = '  tts->impl = std::make_unique<sherpa_onnx::OfflineTts>(tts_config);'
if (-not $upstream.Contains($marker)) { throw 'Sherpa TTS construction changed' }
$guarded = @'
  try {
    tts->impl = std::make_unique<sherpa_onnx::OfflineTts>(tts_config);
  } catch (const std::exception &error) {
    SHERPA_ONNX_LOGE("Could not create OfflineTts: %s", error.what());
    delete tts;
    return nullptr;
  }
'@
$source = $upstream.Replace($marker, $guarded).Replace('#include <cstring>', "#include <cstring>`n#include <exception>") + "`n"
if ([IO.File]::ReadAllText($api) -cne $source) { [IO.File]::WriteAllText($api, $source) }

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
