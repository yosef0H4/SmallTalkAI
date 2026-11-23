"""Inspect the real wizard and test its payload in a registration-free install."""
import argparse
import ctypes as C
from ctypes import wintypes as W
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import tomllib

from PIL import ImageGrab

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "artifacts/windows-validation/installer-parity"
OUT.mkdir(parents=True, exist_ok=True)
u = C.WinDLL("user32", use_last_error=True)
u.SetProcessDpiAwarenessContext(C.c_void_p(-4))
callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
u.GetWindowTextW.argtypes = [W.HWND, W.LPWSTR, C.c_int]
u.GetClassNameW.argtypes = [W.HWND, W.LPWSTR, C.c_int]
u.IsWindowVisible.argtypes = [W.HWND]
u.IsWindowEnabled.argtypes = [W.HWND]
u.GetWindowRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
u.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
u.SendMessageTimeoutW.argtypes = [W.HWND, W.UINT, W.WPARAM, W.LPARAM, W.UINT, W.UINT, C.POINTER(C.c_size_t)]


def label(hwnd, class_name=False):
    text = C.create_unicode_buffer(1024)
    (u.GetClassNameW if class_name else u.GetWindowTextW)(hwnd, text, len(text))
    return text.value


def windows(parent=None):
    result = []

    @callback
    def collect(hwnd, _):
        result.append(hwnd)
        return True

    if parent:
        u.EnumChildWindows(W.HWND(parent), collect, 0)
    else:
        u.EnumWindows(collect, 0)
    return result


def message(hwnd, code, value=0, pointer=0):
    result = C.c_size_t()
    assert u.SendMessageTimeoutW(hwnd, code, value, pointer, 2, 1000, C.byref(result)), "wizard control timed out"
    return result.value


def inspect_wizard(installer, version):
    before = set(windows())
    process = subprocess.Popen([str(installer), "/SP-", "/NORESTART", "/NOICONS", "/SUPPRESSMSGBOXES", f"/DIR={OUT / 'wizard-only'}"])
    wizard = None
    try:
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            candidates = [
                h for h in windows()
                if h not in before and u.IsWindowVisible(h) and label(h, True) == "TWizardForm"
            ]
            if candidates:
                wizard = candidates[0]
                break
            time.sleep(0.1)
        assert wizard, "setup wizard did not appear"
        assert f"version {version}" in label(wizard), label(wizard)
        tasks = None
        for _ in range(8):
            children = [h for h in windows(wizard) if u.IsWindowVisible(h)]
            lists = [h for h in children if label(h, True) == "TNewCheckListBox"]
            if lists:
                tasks = []
                count = message(lists[0], 0x018B)  # LB_GETCOUNT
                assert count < 20, "unexpected task list size"
                for index in range(count):
                    text = C.create_unicode_buffer(1024)
                    message(lists[0], 0x0189, index, C.addressof(text))
                    tasks.append(text.value)
                break
            for hwnd in children:
                if "i accept" in label(hwnd).replace("&", "").lower():
                    message(hwnd, 0x00F5)  # BM_CLICK
            next_buttons = [h for h in children if "Button" in label(h, True) and label(h).replace("&", "").lower().startswith("next") and u.IsWindowEnabled(h)]
            if not next_buttons:
                rect = W.RECT()
                u.GetWindowRect(wizard, C.byref(rect))
                ImageGrab.grab(bbox=(rect.left, rect.top, rect.right, rect.bottom)).save(OUT / "wizard-failure.png")
                raise AssertionError([(label(h, True), label(h), bool(u.IsWindowEnabled(h))) for h in children])
            message(next_buttons[0], 0x00F5)
            time.sleep(0.2)
        assert tasks, "additional tasks page was not reached"
        assert any("desktop shortcut" in task.lower() for task in tasks), tasks
        assert any("sign in" in task.lower() for task in tasks), tasks
        assert not any(re.search("model|download|speech", task, re.I) for task in tasks), tasks
        rect = W.RECT()
        u.GetWindowRect(wizard, C.byref(rect))
        ImageGrab.grab(bbox=(rect.left, rect.top, rect.right, rect.bottom)).save(OUT / "tasks.png")
        (OUT / "wizard.json").write_text(json.dumps({"title": label(wizard), "tasks": tasks}, indent=2))
        print("PASS real setup wizard: Cargo version; only shortcut/startup tasks", flush=True)
    finally:
        # This test never clicks Install. Terminate only this newly opened wizard.
        if wizard:
            pid = W.DWORD()
            u.GetWindowThreadProcessId(wizard, C.byref(pid))
            subprocess.run(["taskkill", "/PID", str(pid.value), "/T", "/F"], capture_output=True, timeout=10)
        if process.poll() is None:
            process.kill()
        process.wait(timeout=5)


def verify_launcher(target):
    launchers = list(target.rglob("SmallTalkAI.exe"))
    assert launchers == [target / "SmallTalkAI.exe"], launchers
    assert not list(target.rglob("*.cmd")), "installer added command-script launchers"
    probe = target.parent / "Launcher Probe With Spaces"
    runtime = probe / "runtime"
    runtime.mkdir(parents=True, exist_ok=True)
    launcher = probe / "SmallTalkAI.exe"
    launcher.write_bytes(launchers[0].read_bytes())
    (runtime / "AutoHotkey64.exe").write_bytes((target / "runtime/AutoHotkey64.exe").read_bytes())
    script = runtime / "smalltalkai.ahk"
    script.write_text(
        '#Requires AutoHotkey v2.0\n#SingleInstance Force\n'
        'FileAppend(A_WorkingDir . "`n" . A_ScriptFullPath . "`n" . A_Args[2] . "`n" . A_Args[3], A_Args[1], "UTF-8-RAW")\n',
        encoding="ascii",
    )
    argument = 'café with "quotes"'
    marker = probe / "launch result.txt"
    for args, expected_flag in [([], "--show-settings"), (["--background"], "--background")]:
        marker.unlink(missing_ok=True)
        subprocess.run([str(launcher), str(marker), argument, *args], cwd=OUT, check=True, timeout=10)
        expected = [str(runtime), str(script), argument, expected_flag]
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            try:
                if marker.read_text(encoding="utf-8").splitlines() == expected:
                    break
            except OSError:
                pass
            time.sleep(0.05)
        else:
            raise AssertionError(f"{launcher} did not forward the arguments for {expected_flag}")
    print("PASS the native launcher forwards foreground/background arguments from a directory with spaces", flush=True)


def install_payload(iscc, version):
    source = (ROOT / "artifacts/smalltalkai.iss").read_text(encoding="utf-8")
    # Keep all install tasks, files and launch logic identical; isolate
    # only registration/output metadata so the user's installation is untouched.
    overrides = {
        "AppId": "{{11B34A39-5E88-47A0-A600-826B9185F0D1}",
        "OutputDir": str(OUT),
        "OutputBaseFilename": "setup-isolated",
        "DefaultGroupName": "SmallTalkAI isolated installer test",
    }
    for key, value in overrides.items():
        source, count = re.subn(rf"(?m)^{key}=.*$", lambda _: f"{key}={value}", source)
        assert count == 1, key
    source = source.replace("[Setup]\n", f"[Setup]\nSourceDir={ROOT / 'artifacts'}\nUninstallable=no\nCreateUninstallRegKey=no\n")
    startup_name = f"SmallTalkAI installer parity {os.getpid()}"
    source = source.replace(r'Name: "{userstartup}\SmallTalkAI";', rf'Name: "{{userstartup}}\{startup_name}";')
    source = source.replace(r'Name: "{group}\SmallTalkAI";', r'Name: "{app}\Test Start Menu Shortcut";')
    test_script = OUT / "setup-isolated.iss"
    test_script.write_text(source, encoding="utf-8")
    with (OUT / "compile.log").open("w") as log:
        subprocess.run([str(iscc), f"/DSmallTalkAIVersion={version}", str(test_script)], stdout=log, stderr=subprocess.STDOUT, check=True, timeout=120)
    with tempfile.TemporaryDirectory(prefix="clean-install-", dir=OUT) as directory:
        target = Path(directory) / "installed SmallTalkAI"
        subprocess.run([str(OUT / "setup-isolated.exe"), "/VERYSILENT", "/SP-", "/SUPPRESSMSGBOXES", "/NORESTART", "/NOICONS", "/TASKS=", f"/DIR={target}", f"/LOG={OUT / 'install.log'}"], check=True, timeout=120)
        verify_installed_payload(target)
    with tempfile.TemporaryDirectory(prefix="startup-install-", dir=OUT) as directory:
        profile = Path(directory)
        target = profile / "installed SmallTalkAI"
        config = profile / "config.json"
        startup_dir = subprocess.check_output(
            ["powershell.exe", "-NoProfile", "-Command", "[Environment]::GetFolderPath('Startup')"],
            encoding="utf-8", timeout=10,
        ).strip()
        shortcut = Path(startup_dir) / f"{startup_name}.lnk"
        assert startup_dir and not shortcut.exists(), shortcut
        env = dict(os.environ, SMALLTALKAI_CONFIG=str(config))
        try:
            subprocess.run(
                [str(OUT / "setup-isolated.exe"), "/VERYSILENT", "/SP-", "/SUPPRESSMSGBOXES",
                 "/NORESTART", "/TASKS=startup", f"/DIR={target}", f"/LOG={OUT / 'startup-install.log'}"],
                env=env, check=True, timeout=120,
            )
            assert shortcut.is_file(), "Setup did not create the current user's Startup shortcut"
            assert config.is_file(), "Setup created the shortcut without saving Start at login"
            assert json.loads(config.read_text(encoding="utf-8"))["general"]["start_at_login"] is True
        finally:
            shortcut.unlink(missing_ok=True)
    print("PASS Setup's Startup task creates the sign-in shortcut and saves Start at login", flush=True)


def verify_installed_payload(target):
    assert not list(target.rglob("*.gguf")), "installer added speech models"
    assert not list(target.rglob("*.onnx")), "installer added TTS/OCR models"
    libraries = {path.relative_to(target) for path in target.rglob("*.dll")
                 if path.name.startswith(("parakeet", "ggml", "onnxruntime", "sherpa-onnx"))}
    bridge = Path("runtime/sherpa-webgpu/sherpa-onnx-c-api.dll")
    bundled = target / "runtime/bundled-runtimes"
    expected = {"parakeet-vulkan-windows", "sherpa-cpu-windows", "paddle-webgpu-windows"}
    assert {path.name for path in bundled.iterdir()} == expected
    assert all((path / ".verified").is_file() for path in bundled.iterdir())
    assert bridge in libraries
    assert all(path == bridge or path.parts[:2] == ("runtime", "bundled-runtimes") for path in libraries), libraries
    assert not any(path.name.startswith(("cudart", "cublas", "cudnn", "curand"))
                   for path in target.rglob("*.dll"))
    assert (target / "runtime/action-replay.ahk").is_file()
    assert (target / "runtime/smalltalkai-settings-window.exe").is_file()
    assert (target / "licenses/settings-sans-license.txt").is_file()
    assert (target / "runtime/smalltalkai.ahk").is_file()
    verify_launcher(target)
    executable = target / "runtime/smalltalkai-ctl.exe"
    env = dict(os.environ, SMALLTALKAI_CONFIG=str(target / "unused-test-config.json"))
    for scope in ["app", "reader"]:
        result = json.loads(subprocess.check_output([str(executable), "config", "--scope", scope, "defaults"], env=env, timeout=15))
        result = result.get("config", result)
        if scope == "app":
            assert not result["speech"]["single_model_filename"]
            assert result["general"]["ui_theme"] == "light"
            assert result["general"]["start_at_login"] is False
            for key in ["toggle_delivery_hotkey", "toggle_cleanup_hotkey", "retry_delivery_hotkey"]:
                assert result["general"][key] == "None", key
            assert result["general"]["show_shortcuts_hotkey"] == "CapsLock"
            assert result["general"]["show_shortcuts_mode"] == "hold"
        else:
            assert all(value == "None" for value in result["shortcuts"].values())
    print("PASS isolated clean install: no models added, packaged defaults stay empty", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--iscc", type=Path, required=True)
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    inspect_wizard(ROOT / "artifacts/dist/smalltalkai-setup.exe", version)
    install_payload(args.iscc, version)
