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
    errors.append("Windows distribution must obtain inference runtimes after installation")
if "SimpleSttVersion" not in setup or "/DSimpleSttVersion=" not in builder:
    errors.append("installer version is not supplied from Cargo")
if re.search(r"^AppVersion=\d", setup, re.M):
    errors.append("installer version is hardcoded")
if "'runtime\\action-replay.ahk'" not in builder:
    errors.append("recorded keyboard action helper is not required in the package")
if "'runtime\\smalltalkai-settings-window.exe'" not in builder:
    errors.append("the native Settings window is not required in the package")
if not all(part in builder for part in ("'SmallTalkAI.exe'", "'runtime\\SmallTalkAI.exe'", "smalltalkai-launcher.exe", "VersionInfo.FileDescription")):
    errors.append("the native launcher, compatibility copy and Windows metadata must be verified")
if "Ahk2Exe" in builder:
    errors.append("the Windows package still compiles the AutoHotkey script")
if "AppName=SmallTalkAI" not in setup or "UsePreviousAppDir=yes" not in setup:
    errors.append("the installer must carry the new name and reuse the existing profile's install directory")
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
