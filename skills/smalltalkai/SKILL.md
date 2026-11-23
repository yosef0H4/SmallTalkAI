---
name: smalltalkai
description: Read aloud or give a spoken task update with SmallTalkAI. Also use for requested WAV export, audio transcription, or image OCR.
---

# SmallTalkAI

<!-- executable -->
Use `smalltalkai` on Linux or `smalltalkai-ctl.exe` on Windows. If the command is unavailable, locate the installed app before continuing.
<!-- /executable -->

Use the user's saved provider, model, voice, and output device. Speak only when requested. In PowerShell, use `&` before a quoted executable path.

- **Spoken result.** Lead with the outcome. Use a few short sentences and say the next action only if there is one. Turn tables into their main takeaway. Skip Markdown, raw URLs, file paths, commands, and log dumps unless the user asks to hear them.
- **Read aloud.** Use the supplied text. Preserve it for verbatim reading and audio export.
- **Play.** Pass short text as one argument to `media speak <text> --background`, or use `media speak --file <path> --background` with a UTF-8 file. Pass paths as separate arguments. If piping text to `--stdin` from PowerShell, first set `$OutputEncoding = [System.Text.UTF8Encoding]::new($false)` in that shell so Windows PowerShell 5.1 preserves non-ASCII characters.
- **After submission.** Continue without waiting for playback. Describe success as a handoff. If submission fails, include the error in your written reply. Launch or retry playback only if the user asks for it.
- **Export.** Run `media speak --file <path> --output <wav-path>` and wait for the file.
- **Transcribe or OCR.** Run `media transcribe <media-path> --json` or `media ocr <image-path> --json`. Transcription needs FFmpeg. Report missing model or runtime assignments so the user can choose them in Settings.
