# Configuration schema and ownership

`config.json` is SmallTalkAI's canonical, portable configuration. The disposable `smalltalkai-settings` browser process is only a visual editor for it. Windows uses `%APPDATA%\smalltalkai\instances\<runtime-id>\config.json`; Linux uses the matching XDG config directory. `SMALLTALKAI_CONFIG` overrides the path for development and tests.

The current schema is version 9:

```json
{
  "schema_version": 10,
  "general": {
    "enabled": true,
    "recording_mode": "hold",
    "record_hotkey": "CapsLock+S",
    "toggle_delivery_hotkey": "CapsLock+D",
    "cancel_hotkey": "CapsLock+A",
    "capslock_behavior": "preserve_tap",
    "start_at_login": false,
    "ui_theme": "auto"
  },
  "audio": { "preferred_device_id": "", "gain": 1.0 },
  "speech": {
    "inference_device": "auto",
    "runtime_dir": "external\\parakeet-runtime\\parakeet-windows-vulkan",
    "model_dir": "external\\parakeet-runtime\\models",
    "selection_mode": "single_model",
    "single_model_filename": null,
    "language_models": {},
    "idle_worker_timeout_secs": 180,
    "worker_shutdown_grace_ms": 2000
  },
  "output": {
    "delivery_mode": "smart_paste",
    "enabled_delivery_modes": ["smart_paste", "type"],
    "app_overrides": [],
    "paced_typing_enabled": true,
    "typing_speed_wpm": 450,
    "trailing_space": true,
    "remove_punctuation": false,
    "lowercase": false
  },
  "diagnostics": { "log_level": "normal", "diagnostic_overlay": false, "log_transcripts": false }
}
```

Every parseable file is normalized immediately: exact current fields with valid values are retained, missing or invalid fields receive defaults, and unknown/obsolete fields are dropped. This first release uses the current schema only; explicit unsupported schema versions are rejected without rewriting the file; obsolete model-selection fields are ignored and no runtime/model paths or files are migrated. Current model assignments, including missing filenames, are retained. A syntactically malformed file is preserved byte-for-byte and Settings requires an explicit Import or Reset preview followed by Save before replacing it.

`speech.selection_mode` is `single_model` (Use one model) or `follow_keyboard`.
Fresh installations follow the keyboard, with no assigned models.
Follow keyboard uses `language_models`, mapping language IDs such as `en`, `ar`,
and `fr` to installed filenames or `null`. Regional keyboard layouts share one
ISO language ID. Unsupported layout IDs remain stable identifiers for manual
assignment. Saved None and missing files silently skip dictation before capture,
overlay, screenshots, or model warm-up. The chosen model and device are frozen
at recording start; changing language alone does not recycle a worker using the
same model.

Windows discovers loaded layouts and locale language codes. Linux reads KDE
Plasma's configured and active layouts on Wayland, or configured XKB layouts and
the effective group on X11. Installed XKB rules XML supplies language metadata,
including variant overrides. Other Wayland desktops show discovery as unavailable;
use one model there. XWayland never supplies a native Wayland target's language.

Audio & recognition contains searchable installed-model choices and model tests.
Every choice includes None and permits any installed model. Follow keyboard ranks
declared language matches first. Refresh languages edits only the draft: new
languages receive a declared match, ordered by recommended, Q8, size, filename,
or None. Existing assignments, including None and removed languages, are retained.
Save activates choices. Model installer handles downloads and removal independently;
installing a model never replaces an assignment, and saved assignments block removal.

`speech.inference_device` is `auto`, `gpu`, or `cpu`. Vulkan GPU selection prefers a physical discrete GPU.

Writes use a temporary file, flush it, and atomically replace the destination. Relative runtime/model paths are preserved in JSON and resolved against the runtime root only when used. An unavailable `audio.preferred_device_id` is retained while capture temporarily follows the system default.

`output.delivery_mode` is the method used immediately. On both platforms,
`output.enabled_delivery_modes` lists the methods in shortcut cycle order.
The active method can be outside the cycle. Include at least one method.
Linux starts with typing and Windows starts with Smart Paste. Typing defaults
to 450 WPM. Finite speeds span 50–1650 WPM. Instant uses
`paced_typing_enabled: false` and keeps the saved finite speed.

The settings server detects external edits with a content hash before Save. A successful Save asks the capture service to reload and emits `configuration_reloaded`; Windows AHK then reapplies its owned hotkeys, startup registration, transforms, and delivery settings.

On Windows and Linux, GPU mode selects a physical Vulkan GPU, preferring a discrete GPU over integrated graphics. It rejects CPU/software Vulkan devices and errors if no physical GPU is available. Automatic mode uses the same preference with CPU fallback. CPU mode always uses the CPU.

## Interface language

`general.ui_language` accepts `auto` (default), `en`, or `ar` within schema 10. Missing/invalid values become `auto`. This setting is independent of keyboard layout, speech model assignments, and transcript content. Settings previews changes immediately; Save applies them to desktop menus and capture notices. System uses Windows user UI language or Linux message-locale preferences, with English fallback.

Canonical messages live in `web/settings/src/lib/locales/`: `en.json`/`ar.json` for Settings and `desktop.en.json`/`desktop.ar.json` for native UI. Use stable keys and named placeholders. `node web/settings/tools/locales.mjs` validates key/placeholder parity and generates `ahk/lib/Locale.ahk`; `npm run build` runs it automatically. Rust embeds the same desktop catalogs. Do not edit the generated AHK catalog directly.

Local reading settings start with a mode. One-model mode edits one assignment; language modes show a language selector and one assignment editor. Each configured language has its own assignment; unassigned languages do not inherit another one. Each reading assignment owns its runtime, model, voice, device, cleanup and optional speed multiplier. Selecting another language or mode preserves the other assignments. A missing local assignment speed uses 1×.

The default finish delay is 500 ms. Existing saved custom delays are preserved. The local reading tooltip shows the current prepared passage; system notification text remains subject to the notification privacy setting.

## Generate a server token over SSH

`smalltalkai credentials generate server` prints the existing shared token or securely generates and saves one in the credential vault. `--replace` rotates it; restart the server and update clients afterward. One token authenticates every media endpoint.

On a headless host without a keyring, generate to a private file and explicitly supply it to the server:

```sh
umask 077
smalltalkai credentials generate server --stdout > ~/smalltalkai-server.token
export SMALLTALKAI_SERVER_TOKEN="$(cat ~/smalltalkai-server.token)"
smalltalkai serve
```

Generate the file once and reuse it for later starts. `--stdout` never reads the keyring or saves anything itself. It prints only the new token. An environment override prevents accidental vault rotation; unset it to manage the vault token.
