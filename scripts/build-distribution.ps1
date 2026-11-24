param(
    [switch]$SkipTests,
    [string]$AhkBase,
    [string]$Iscc,
    [string]$CargoTargetDir,
    [ValidatePattern('^[0-9A-Fa-f]{40}$')]
    [string]$SigningCertificateThumbprint,
    [string]$SignTool
)
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
$PackageVersionMatch = [regex]::Match((Get-Content -LiteralPath (Join-Path $Root 'Cargo.toml') -Raw), '(?m)^version\s*=\s*"([^"]+)"')
if (-not $PackageVersionMatch.Success) { throw 'Unable to read package version from Cargo.toml' }
$PackageVersion = $PackageVersionMatch.Groups[1].Value
$Artifacts = Join-Path $Root 'artifacts'
$Portable = Join-Path $Artifacts 'smalltalkai-portable'
$Dist = Join-Path $Artifacts 'dist'
$Runtime = Join-Path $Portable 'runtime'
$ResolvedTargetDir = if ($CargoTargetDir) {
    [System.IO.Path]::GetFullPath($CargoTargetDir)
} elseif ($env:CARGO_TARGET_DIR) {
    [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
} else {
    Join-Path $Root 'target'
}

function Require-File([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Required file is missing: $Path" }
}
function Get-Sha256([string]$Path) {
    $Stream = [System.IO.File]::OpenRead($Path)
    try {
        $Sha = [System.Security.Cryptography.SHA256]::Create()
        try {
            (($Sha.ComputeHash($Stream) | ForEach-Object { $_.ToString('x2') }) -join '').ToUpperInvariant()
        } finally {
            $Sha.Dispose()
        }
    } finally {
        $Stream.Dispose()
    }
}
function Resolve-Tool([string]$Provided, [string[]]$Candidates, [string]$Label) {
    if ($Provided) { Require-File $Provided; return $Provided }
    foreach ($Candidate in $Candidates) { if (Test-Path -LiteralPath $Candidate -PathType Leaf) { return $Candidate } }
    throw "$Label was not found. Pass its path explicitly."
}
function Sign-AppFile([string]$Path) {
    & $SignTool sign /sha1 $SigningCertificateThumbprint /fd SHA256 /tr https://timestamp.digicert.com /td SHA256 $Path
    if ($LASTEXITCODE -ne 0) { throw "Signing failed for $Path" }
    & $SignTool verify /pa $Path
    if ($LASTEXITCODE -ne 0) { throw "Signature verification failed for $Path" }
}
if ($SigningCertificateThumbprint) {
    $InstalledSignTool = Get-Command signtool.exe -ErrorAction SilentlyContinue
    $SignToolCandidates = if ($InstalledSignTool) { @($InstalledSignTool.Source) } else { @() }
    $SignTool = Resolve-Tool $SignTool $SignToolCandidates 'Windows SDK SignTool (signtool.exe)'
}
$AhkBase = Resolve-Tool $AhkBase @(
    "$env:ProgramFiles\AutoHotkey\v2\AutoHotkey64.exe",
    "$env:ProgramFiles\AutoHotkey\v2\AutoHotkey.exe",
    "$env:LOCALAPPDATA\Programs\AutoHotkey\v2\AutoHotkey64.exe",
    "$env:LOCALAPPDATA\Programs\AutoHotkey\v2\AutoHotkey.exe"
) 'AutoHotkey v2 base runtime'
$Iscc = Resolve-Tool $Iscc @(
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) 'Inno Setup compiler (ISCC.exe)'
Require-File (Join-Path $Root 'ahk\smalltalkai.ahk')
Require-File (Join-Path $Root 'ahk\hotkey-recorder.ahk')
Require-File (Join-Path $Root 'ahk\action-replay.ahk')
Require-File (Join-Path $Root 'fixtures\parakeet-smoke.wav')
Require-File (Join-Path $Root 'fixtures\asr\arabic-sa.wav')
$BuildRelease = Join-Path $PSScriptRoot 'build-release.ps1'
if ($SkipTests) {
    & $BuildRelease -SkipTests -CargoTargetDir $ResolvedTargetDir
} else {
    & $BuildRelease -CargoTargetDir $ResolvedTargetDir
}
$SherpaWebGpu = & (Join-Path $PSScriptRoot 'build-sherpa-webgpu.ps1')

if (Test-Path -LiteralPath $Portable) { Remove-Item -LiteralPath $Portable -Recurse -Force }
New-Item -ItemType Directory -Path $Runtime -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $Root 'ahk\smalltalkai.ahk') -Destination $Runtime -Force
Copy-Item -LiteralPath (Join-Path $Root 'ahk\hotkey-recorder.ahk') -Destination $Runtime -Force
Copy-Item -LiteralPath (Join-Path $Root 'ahk\action-replay.ahk') -Destination $Runtime -Force
Copy-Item -LiteralPath (Join-Path $Root 'ahk\lib') -Destination (Join-Path $Runtime 'lib') -Recurse -Force
Copy-Item -LiteralPath (Join-Path $Root 'ahk\icons') -Destination (Join-Path $Runtime 'icons') -Recurse -Force
Copy-Item -LiteralPath $AhkBase -Destination (Join-Path $Runtime 'AutoHotkey64.exe') -Force
$DesktopExe = Join-Path $Portable 'SmallTalkAI.exe'
Copy-Item -LiteralPath (Join-Path $ResolvedTargetDir 'release\smalltalkai-launcher.exe') -Destination $DesktopExe -Force
Require-File $DesktopExe
if ((Get-Item -LiteralPath $DesktopExe).VersionInfo.FileDescription -ne 'SmallTalkAI') {
    throw 'SmallTalkAI.exe has incorrect process metadata'
}

foreach ($Name in @('smalltalkai-capture.exe','smalltalkai-infer.exe','smalltalkai-ctl.exe','smalltalkai-settings.exe','smalltalkai-settings-window.exe','smalltalkai-reader.exe','smalltalkai-readerctl.exe','smalltalkai-tts.exe','smalltalkai-ocr.exe','smalltalkai-reader-hud.exe')) {
    Copy-Item -LiteralPath (Join-Path $ResolvedTargetDir "release\$Name") -Destination $Runtime -Force
}
$BundledRuntimeIds = @('parakeet-vulkan-windows', 'sherpa-cpu-windows', 'paddle-webgpu-windows')
$BundledCache = Join-Path $Root 'artifacts/bundled-runtimes-windows'
& (Join-Path $Runtime 'smalltalkai-ctl.exe') assets stage-bundled $BundledCache
if ($LASTEXITCODE -ne 0) { throw 'Could not stage verified bundled runtimes' }
$BundledRuntimes = Join-Path $Runtime 'bundled-runtimes'
New-Item -ItemType Directory -Path $BundledRuntimes -Force | Out-Null
foreach ($Id in $BundledRuntimeIds) {
    Copy-Item -LiteralPath (Join-Path $BundledCache $Id) -Destination $BundledRuntimes -Recurse -Force
    Require-File (Join-Path $BundledRuntimes "$Id/.verified")
}
$SherpaDirectory = Join-Path $Runtime 'sherpa-webgpu'
New-Item -ItemType Directory -Path $SherpaDirectory -Force | Out-Null
Copy-Item -LiteralPath $SherpaWebGpu -Destination $SherpaDirectory -Force
$SherpaSource = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $SherpaWebGpu))
Copy-Item -LiteralPath (Join-Path $SherpaSource 'LICENSE') -Destination (Join-Path $SherpaDirectory 'Sherpa-ONNX-LICENSE.txt')
Copy-Item -LiteralPath (Join-Path $SherpaSource 'build/_deps/espeak_ng-src/COPYING') -Destination (Join-Path $SherpaDirectory 'eSpeak-NG-COPYING.txt')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'build-sherpa-webgpu.ps1') -Destination $SherpaDirectory
Copy-Item -LiteralPath (Join-Path $Root 'assets\language\lid.176.ftz') -Destination $Runtime -Force
Copy-Item -LiteralPath (Join-Path $Root 'LICENSE') -Destination $Portable -Force
New-Item -ItemType Directory -Path (Join-Path $Portable 'licenses') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $Root 'assets\language\LICENSE-CC-BY-SA-3.0.txt') -Destination (Join-Path $Portable 'licenses') -Force
Copy-Item -LiteralPath (Join-Path $Root 'assets\language\ATTRIBUTION.md') -Destination (Join-Path $Portable 'licenses\fastText-ATTRIBUTION.md') -Force
Copy-Item -LiteralPath (Join-Path $Root 'assets\fonts\LICENSE-NotoSansArabic.txt') -Destination (Join-Path $Portable 'licenses') -Force
Copy-Item -LiteralPath (Join-Path $Root 'web\settings\public\settings-sans-license.txt') -Destination (Join-Path $Portable 'licenses') -Force
Copy-Item -LiteralPath (Join-Path $Root 'assets\fonts\LICENSE-JetBrainsMono.txt') -Destination (Join-Path $Portable 'licenses') -Force
Copy-Item -LiteralPath (Join-Path $Root 'THIRD_PARTY_NOTICES.md') -Destination $Portable -Force
Copy-Item -LiteralPath (Join-Path $Root 'START_HERE.txt') -Destination $Portable -Force
New-Item -ItemType Directory -Path (Join-Path $Runtime 'fixtures') -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $Root 'fixtures\parakeet-smoke.wav') -Destination (Join-Path $Runtime 'fixtures') -Force
Copy-Item -LiteralPath (Join-Path $Root 'fixtures\asr') -Destination (Join-Path $Runtime 'fixtures\asr') -Recurse -Force
$ModelFiles = @(Get-ChildItem -LiteralPath $Portable -File -Recurse | Where-Object { $_.Extension -in @('.gguf', '.onnx', '.safetensors', '.tflite', '.pt', '.pth') })
if ($ModelFiles.Count) { throw 'Distribution payload unexpectedly contains an inference model' }
$PackagedSherpa = Join-Path $SherpaDirectory 'sherpa-onnx-c-api.dll'
$InferenceLibraries = @(Get-ChildItem -LiteralPath $Portable -File -Recurse -Filter '*.dll' | Where-Object {
    $LibraryPath = $_.FullName
    $InBundledRuntime = $BundledRuntimeIds | Where-Object {
        $Prefix = (Join-Path $BundledRuntimes $_) + [IO.Path]::DirectorySeparatorChar
        $LibraryPath.StartsWith($Prefix, [StringComparison]::OrdinalIgnoreCase)
    }
    $_.Name -match '^(parakeet|ggml|onnxruntime|sherpa-onnx|cudart|cublas|cudnn|curand)' -and
        $_.FullName -ne $PackagedSherpa -and -not $InBundledRuntime
})
if ($InferenceLibraries.Count) { throw 'Distribution payload unexpectedly contains an inference runtime' }
$Required = @(
    'runtime\smalltalkai.ahk',
    'runtime\hotkey-recorder.ahk',
    'runtime\action-replay.ahk',
    'runtime\icons\smalltalkai.ico',
    'runtime\icons\book.ico',
    'runtime\icons\LICENSE-NOTO-EMOJI.txt',
    'runtime\AutoHotkey64.exe',
    'SmallTalkAI.exe',
    'runtime\smalltalkai-capture.exe',
    'runtime\smalltalkai-infer.exe',
    'runtime\smalltalkai-ctl.exe',
    'runtime\smalltalkai-settings.exe',
    'runtime\sherpa-webgpu\sherpa-onnx-c-api.dll',
    'runtime\smalltalkai-settings-window.exe',
    'runtime\smalltalkai-reader.exe',
    'runtime\smalltalkai-readerctl.exe',
    'runtime\smalltalkai-tts.exe',
    'runtime\smalltalkai-ocr.exe',
    'runtime\smalltalkai-reader-hud.exe',
    'runtime\fixtures\parakeet-smoke.wav'
)
foreach ($RelativePath in $Required) { Require-File (Join-Path $Portable $RelativePath) }
if ($SigningCertificateThumbprint) {
    foreach ($RelativePath in $Required) {
        if ($RelativePath -match '(^SmallTalkAI\.exe$|^runtime\\smalltalkai-.*\.exe$)') {
            Sign-AppFile (Join-Path $Portable $RelativePath)
        }
    }
}

New-Item -ItemType Directory -Path $Dist -Force | Out-Null
$Setup = Join-Path $Dist 'smalltalkai-setup.exe'
$Zip = Join-Path $Dist 'smalltalkai-portable.zip'
$InnoScript = Join-Path $Artifacts 'smalltalkai.iss'
Copy-Item -LiteralPath (Join-Path $Root 'resources\smalltalkai.iss') -Destination $InnoScript -Force
foreach ($Output in @($Setup, $Zip)) {
    if (Test-Path -LiteralPath $Output) { Remove-Item -LiteralPath $Output -Force }
}
& $Iscc "/DSmallTalkAIVersion=$PackageVersion" $InnoScript
if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
Require-File $Setup
if ($SigningCertificateThumbprint) { Sign-AppFile $Setup }
Get-ChildItem -LiteralPath $Portable | Compress-Archive -DestinationPath $Zip -CompressionLevel Optimal
Require-File $Zip
Write-Host ''
Write-Host 'Distribution build complete:'
foreach ($Output in @($Setup, $Zip)) {
    Write-Host ('  ' + $Output)
    Write-Host ('  SHA256 ' + (Get-Sha256 $Output))
}
