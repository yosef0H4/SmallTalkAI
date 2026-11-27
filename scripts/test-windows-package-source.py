"""Keep the Windows installer model-free and preserve user-owned data."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
setup = (ROOT / "resources/smalltalkai.iss").read_text(encoding="utf-8")
builder = (ROOT / "scripts/build-distribution.ps1").read_text(encoding="utf-8")
errors = []
for forbidden in ["external download", "https://", "downloadmodel", "installarabic", "ModelNeedsDownload"]:
    if forbidden.lower() in setup.lower():
        errors.append("installer still contains " + forbidden)
for line in setup.splitlines():
    if line.startswith("Type:") and any(part in line.lower() for part in ["models", "appdata", "userstartup", "autodesktop", "userprograms"]):
        errors.append("installer deletes user-owned data: " + line)
if "IncludeModel" in builder:
    errors.append("distribution builder still supports embedding speech models")
if "InferenceLibraries" not in builder or "ParakeetSource" in builder:
    errors.append("Windows distribution must reject unintended inference runtimes")
if 'assets stage-bundled' not in builder or not all(runtime in builder for runtime in (
        'parakeet-vulkan-windows', 'sherpa-cpu-windows', 'paddle-webgpu-windows')):
    errors.append("Windows distribution must ship verified Vulkan, CPU, and WebGPU runtimes")
if 'build-dawn-runtime.py' not in builder or builder.find('build-dawn-runtime.py') < builder.find('assets stage-bundled'):
    errors.append("Windows distribution must replace the stock WebGPU DLL with the source-built Dawn runtime")
if "SmallTalkAIVersion" not in setup or "/DSmallTalkAIVersion=" not in builder:
    errors.append("installer version is not supplied from Cargo")
if re.search(r"^AppVersion=\d", setup, re.M):
    errors.append("installer version is hardcoded")
if "'runtime\\action-replay.ahk'" not in builder:
    errors.append("recorded keyboard action helper is not required in the package")
if "'runtime\\smalltalkai-settings-window.exe'" not in builder:
    errors.append("the native Settings window is not required in the package")
if not all(part in builder for part in ("'SmallTalkAI.exe'", "smalltalkai-launcher.exe", "VersionInfo.FileDescription")):
    errors.append("the native launcher and its Windows metadata must be verified")
if "'runtime\\SmallTalkAI.exe'" in builder or ".cmd')" in builder:
    errors.append("the distribution must ship a single native launcher")
if "Ahk2Exe" in builder:
    errors.append("the Windows package still compiles the AutoHotkey script")
if "AppName=SmallTalkAI" not in setup or "UsePreviousAppDir=no" not in setup or "[InstallDelete]" in setup:
    errors.append("the installer must use the current name and a clean installation directory")
icon = r"runtime\icons\smalltalkai.ico"
if f"'{icon}'" not in builder or f"SetupIconFile=smalltalkai-portable\\{icon}" not in setup:
    errors.append("the branded Windows icon is missing from the package or setup")
if not all(f"IconFilename: \"{{app}}\\{icon}\"" in line for line in setup.splitlines() if line.startswith('Name: "{') and 'Filename:' in line):
    errors.append("an installed shortcut is missing the branded Windows icon")
entry_points = [line for line in setup.splitlines() if line.startswith('Filename:') or (line.startswith('Name: "{') and 'Filename:' in line)]
if len(entry_points) != 4 or any(r'Filename: "{app}\SmallTalkAI.exe"' not in line for line in entry_points):
    errors.append("installed shortcuts and post-install launch must use the native launcher")
if errors:
    raise SystemExit("FAIL Windows package policy:\n - " + "\n - ".join(errors))
print("PASS Windows installer: no model downloads/bundles, user data preserved, Cargo version and recorded-action helper required")
