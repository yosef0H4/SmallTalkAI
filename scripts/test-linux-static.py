#!/usr/bin/env python3
"""Static checks for the Linux/Wayland overhaul.

These checks intentionally avoid Rust toolchain requirements so they can run in
minimal CI containers. They do not replace `cargo test --all-targets`.
"""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
errors: list[str] = []

frontend_check = subprocess.run(
    [sys.executable, str(ROOT / "scripts/verify-settings-frontend.py")],
    cwd=ROOT,
    capture_output=True,
    text=True,
)
if frontend_check.returncode:
    print(frontend_check.stdout, end="")
    print(frontend_check.stderr, end="")
    errors.append("Settings frontend build output is stale or exceeds its size budget")


def need(path: str, *needles: str) -> str:
    p = ROOT / path
    if not p.exists():
        errors.append(f"missing {path}")
        return ""
    body = p.read_text(encoding="utf-8")
    for needle in needles:
        if needle not in body:
            errors.append(f"{path} missing {needle!r}")
    return body


need("Cargo.toml", 'cpal = "0.17"', 'libloading = "0.8"')
need(
    "src/capture/audio.rs",
    'cfg(any(windows, target_os = "linux"))',
    'cpal::default_host',
    'choose_input_device_id',
    '.filter(|device| device.id().is_ok_and',
    'cfg(target_os = "linux")',
)
need("src/infer/parakeet_native.rs", "select_native_gpu_device", "preferred_gpu_device", "ggml_backend_dev_type", "GPU mode requires an available physical GPU", 'libparakeet.so', 'parakeet.so', 'parakeet_capi_load')
need("src/capture/inference_supervisor.rs", 'LD_LIBRARY_PATH', 'DYLD_LIBRARY_PATH', 'add_native_library_search_env')
need("src/config.rs", 'CONFIG_SCHEMA_VERSION: u32 = 10', 'pub struct GeneralConfig', 'pub struct AudioConfig', 'language_runtimes', 'parakeet_native_library_candidates', 'screen context requires AI cleanup')
need("src/capture/input_language.rs", "org.kde.KeyboardLayouts", "getLayoutsList", "getLayout", "xkb::get_state", "_XKB_RULES_NAMES", "language_from_layout", "live_keyboard_language")
need("src/capture/process.rs", 'use anyhow::Context;', 'use anyhow::Result;', 'kill')
need("Cargo.toml", 'name = "smalltalkai-linux"', 'path = "src/bin/smalltalkai_linux.rs"')
linux_shell = need(
    "src/bin/smalltalkai_linux.rs",
    'name = "smalltalkai-linux"',
    'Toggle',
    'InstallUserService',
    'ConfigureShortcuts',
    'GlobalShortcuts',
    'portal_shortcuts_loop',
    'wl-copy',
)
if 'smalltalkai-settings.desktop' in linux_shell:
    errors.append("The user service installer must expose Settings through the tray")
need("src/capture/overlay_model.rs", 'ascii_visualizer', 'render_overlay_text', 'OverlayPrimary', 'RecordingIndicators')
need("src/capture/overlay_windows.rs", 'windows_tooltip_text', 'crate::capture::overlay::overlay_model::ascii_visualizer')
need("resources/smalltalkai-paste.c", 'adapted from OpenWhispr', 'PASTE_MODE_SHIFT_INSERT', '--detect-terminal', '--active-app', 'active_app_atspi', 'RemoteDesktop', '--ctrl-v', 'force_ctrl_v ? PASTE_MODE_CTRL_V')
need("src/bin/smalltalkai_linux.rs", 'desktop_input', 'send_paste_key', '"kind":"text"')
need("reader/crates/smalltalkai-reader/src/read_along_capture/kde_backend.rs", 'workspace.stackingOrder', 'crate::portal_input::run', 'fn input_ready', 'smalltalkai-kwin-query.lock', 'unload_script')
need("reader/crates/smalltalkai-reader/src/portal_input.rs", 'DeviceType::Keyboard', 'PersistMode::ExplicitlyRevoked', 'restore_token', 'KeyState::Released', 'connect_to_eis', 'canceled')
need("scripts/build-smalltalkai-paste.py", 'libx11 development packages missing', 'HAVE_GIO')
need("src/common/clipboard.rs", "output_bounded", "--foreground", "text/plain;charset=utf-8", "owner.verify()", "dictation superseded", "MAX_READ")
need("src/bin/smalltalkai_linux.rs", "acquire_clipboard_lock", "file.try_lock()", "invalidate_delivery_session", "clipboard.verify()", "transcript retained in clipboard")
if "fn read_clipboard(" in linux_shell or "from_millis(80)" in linux_shell or 'Ok("pasted")' in linux_shell:
    errors.append("Linux paste must verify publication and must not restore previous text on a timer")
need("scripts/test-linux-paste.py", "-displayfd", "SMALLTALKAI_PASTE_X11_E2E", '"WAYLAND_DISPLAY", "WAYLAND_SOCKET", "AT_SPI_BUS_ADDRESS"', 'XDG_RUNTIME_DIR=str(runtime)', 'DBUS_SESSION_BUS_ADDRESS=', "x11_delayed_paste_end_to_end")
need("src/bin/smalltalkai_linux.rs", "RetryShortcutGate", "wait_retry_modifiers", "ShellCommand::BeginDelivery", "notice.retryEmpty", "notice.retryFailed", "DETECTABLE_AUTO_REPEAT")
need("docs/linux-wayland.md", 'Shared shortcut fields remain in JSON', 'Same Parakeet backend model', 'smalltalkai configure-shortcuts')

need("src/common/clipboard.rs", 'publish_private', '--sensitive')
need("src/common/private_clipboard.rs", 'x-kde-passwordManagerHint', 'INCR', 'MAX_PAYLOAD')
need("src/bin/smalltalkai_linux.rs", 'LinuxCommand::Repeat', 'shortcuts::ACTIONS', 'desktop_input', 'memory_delivery_command')
need("src/shortcuts.rs", 'id: "retry"', 'retry_delivery_hotkey')
helper_source = need("resources/smalltalkai-paste.c", 'ATSPI_ROLE_PASSWORD_TEXT', 'g_dbus_connection_call_sync')
for obsolete in ['XTestFakeKeyEvent', 'paste_via_uinput', 'insert_text_direct', 'activate_window(']:
    if obsolete in helper_source:
        errors.append(f"Linux helper retains forbidden input path: {obsolete}")
for path in ['src/bin/smalltalkai_linux.rs', 'reader/crates/smalltalkai-reader/src/main.rs', 'reader/crates/smalltalkai-reader/src/read_along_capture/kde_backend.rs']:
    body = (ROOT / path).read_text().split("#[cfg(test)]\nmod tests")[0]
    if re.search(r'(?:Command|ProcessCommand)::new\("(?:ydotool|wtype|xdotool)"\)', body):
        # xdotool may be used only for read-only active-window observation.
        if path != 'reader/crates/smalltalkai-reader/src/main.rs':
            errors.append(f"{path} retains a legacy input executable")
need("scripts/test-linux-clean-clipboard.py", 'test_retry', 'application/x-smalltalkai-test', 'UTF8_STRING', 'empty retry cancelled work')
need("src/common/wayland_clipboard.rs", 'x-kde-passwordManagerHint', 'read_finished', 'state.cancelled.contains(&(epoch, true))', 'MAX_TOTAL', 'clipboard changed during backup', 'read_backups', 'BACKUP_LIMIT', 'BACKUP_STALL_LIMIT', 'pending formats:', 'serve_cache_daemon', 'proxy_stdio', 'read_bounded_line', 'SO_PEERCRED', 'cached_offer')
need("src/bin/smalltalkai_linux.rs", 'preserving_wayland_paste', 'ClipboardTransaction', 'notice.clipboardPreserveUnavailable', 'retry_feedback_does_not_claim_clipboard_fallback_was_pasted', 'wayland_clipboard::BACKUP_LIMIT')
need("src/bin/smalltalkai_linux.rs", "wait_record_release", "record_release_gate", "start_clipboard_notice_observer", "clipboard_buffer_notices", "paste stage=waiting-for-read")
need("src/common/wayland_clipboard.rs", "CachePhase", "publish_event", "status.events.len() == 32", "next.events = std::mem::take", "instance: std::process::id()")
need("scripts/test-settings-clipboard-notices.cjs", "clipboard_buffer_notices")
need("scripts/test-linux-unicode-input.py", '--virtual', 'DBUS_SESSION_BUS_ADDRESS', 'application/x-smalltalkai-e2e', 'image/png', 'clipboard_cache_regression', '--cache-only', 'ready_latency_limit')

# Host Registry requires a desktop entry matching the registered app ID.
portal_app_id = re.search(
    r'const LINUX_DESKTOP_ID: &str = "([^"]+)";', (ROOT / "src/config.rs").read_text()
)
if not portal_app_id:
    errors.append("Linux portal app ID is missing")
else:
    desktop = f"packaging/linux/{portal_app_id[1]}.desktop"
    need(desktop, 'Exec=smalltalkai launch')
    need("packaging/linux/container-build.sh", desktop)
    need("packaging/linux/fedora/smalltalkai.spec", f"/usr/share/applications/{portal_app_id[1]}.desktop")

readerd_desktop = need(
    "packaging/linux/smalltalkai-reader.desktop",
    "Exec=/usr/lib/smalltalkai/smalltalkai-reader",
    "NoDisplay=true",
    "X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2",
)
need("packaging/linux/container-build.sh", "smalltalkai-reader.desktop")
need("packaging/linux/fedora/smalltalkai.spec", "/usr/share/applications/smalltalkai-reader.desktop")
reader_identity = need("reader/crates/smalltalkai-reader/src/portal_identity.rs", "io.github.yosef0H4.smalltalkai", "register_host_app")
need("packaging/linux/io.github.yosef0H4.smalltalkai.desktop", "Name=SmallTalkAI", "Icon=smalltalkai")
if portal_app_id and reader_identity and portal_app_id[1] not in reader_identity:
    errors.append("smalltalkai-reader and the Linux shell must register the same portal app ID")
readerd_main = need("reader/crates/smalltalkai-reader/src/main.rs", "portal_identity::register()", "ensure_host_desktop_entry")
if "fn start_desktop_integrations" in readerd_main:
    desktop_setup = readerd_main.split("fn start_desktop_integrations", 1)[1].split("fn read_selection_hotkey", 1)[0]
    if "portal_identity::register()" not in desktop_setup:
        errors.append("smalltalkai-reader must register portal identity before starting desktop integrations")
if "ashpd::register_host_app(" in readerd_main:
    errors.append("smalltalkai-reader must not re-register the portal identity in a portal retry loop")
need("reader/crates/smalltalkai-reader/src/read_along_capture.rs", "kde_backend::KdeBackend")
need("reader/crates/smalltalkai-reader/src/read_along_capture/kde_backend.rs", "CaptureWindow", "slurp", "portal_input", "active_uuid")

# Installed packages must keep mutable data outside their read-only runtime.
need("src/config.rs", 'root.join("linux-installation").is_file()', 'return "installed".to_owned()', 'data_home.join("smalltalkai").join("models")')
need("packaging/linux/smalltalkai-linux.service", 'ExecStart=/usr/bin/smalltalkai daemon', 'WantedBy=graphical-session.target')
package_build = need("packaging/linux/container-build.sh", '--locked', 'linux-installation', 'assets/catalog/downloads.json', '-p smalltalkai-reader', 'atspi_', 'g_dbus_', 'XTestFakeKeyEvent', 'scripts/prepare-linux-package-source.py')
need("packaging/linux/container-build.sh", 'fixtures/parakeet-smoke.wav', 'fixtures/asr/arabic-sa.wav')
need("scripts/test-full.sh", 'python scripts/test-linux-package-source.py')
if '--bins' in package_build or 'smalltalkai_mock_infer' in package_build:
    errors.append("Linux packages must explicitly select production binaries")
need("scripts/test-linux-package.py", 'bundled-runtimes', 'instances/installed/config.json', 'data/smalltalkai/models', '--headless')

# Public source snapshots must exclude local agent permissions and coverage data.
need("scripts/package-linux-release.sh", "--exclude='./.claude'", "--exclude='./lcov.info'", "--exclude='./.env'")


need("src/bin/smalltalkai_capture.rs", "begin_delivery_session", "OverlayPrimary::WaitingForKeys")
need("src/common/shell_protocol.rs", 'DeliveryProgress', 'PreparingClipboard', 'WaitingForPaste')
need("src/bin/smalltalkai_linux.rs", 'memory_delivery_command_with_timeout', 'SmartPaste => smart_paste_key', 'assert_eq!(smart_paste_key(false), PasteKey::CtrlV)')
need("scripts/test-linux-unicode-input.py", 'zero PRIMARY transfers', 'Smart Paste displaced original PRIMARY owner')

need("src/infer/parakeet_native.rs", 'pcm_with_end_context', 'END_CONTEXT_SAMPLES', 'end_context_keeps_the_complete_original_audio')
need("src/bin/smalltalkai_linux.rs", 'DeliveryPhase::Typing')
need("scripts/test-inference-end-context.py", 'expected_last_word', 'struct.pack')

need("src/assets.rs", "Sha256", "Unsafe archive path", "Download cancelled", "Activating verified asset")
need("src/reader.rs", "SMALLTALKAI_EMBEDDED_READER", "SMALLTALKAI_READER_DATA")

if errors:
    for error in errors:
        print(f"FAIL: {error}")
    sys.exit(1)

print("PASS: Linux static overhaul checks")
