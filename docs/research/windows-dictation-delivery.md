# Windows dictation delivery

Source review on 2026-11-23. The cases below concern text insertion after successful transcription. Test with the dictation tool and destination at the same Windows integrity level. Windows restricts synthetic input to processes at an equal or lower integrity level. [Microsoft SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput#remarks)

## Open reports

- An OpenWhispr user reported that transcripts over roughly 150–200 characters failed to auto-paste in Notepad, Windows Terminal, and the VS Code editor and integrated terminal in v1.7.2. Manual paste worked when the transcript was kept on the clipboard. The linked timing change remains open. The report does not establish the same behavior in v1.12.0. [Report #829](https://github.com/OpenWhispr/openwhispr/issues/829) · [Proposed change #1217](https://github.com/OpenWhispr/openwhispr/pull/1217) · [v1.12.0](https://github.com/OpenWhispr/openwhispr/releases/tag/v1.12.0)
- Handy’s wrong-clipboard bug remains open, with reports across multiple platforms and versions. Users describe an older clipboard value landing where the new transcript should go. Reproduce on the current release before attributing it to a particular Windows app or clipboard timing mechanism. [Report #502](https://github.com/cjpais/Handy/issues/502)
- A Codex user reports that Windows global dictation silently fails to insert into Notepad when PowerToys swaps left Ctrl and Alt. The user isolated the failure to the synthesized paste chord. SmallTalkAI has not been checked with the same remap. [Report #56115](https://github.com/openai/codex/issues/56115)

## Compatibility cases

- Handy documents `Shift+Insert` for Windows Terminal, Command Prompt, and PowerShell, and offers Direct Input for fields that do not accept paste. The older Direct Input terminal report was closed with a recommendation to select a different delivery method. [Handy paste methods](https://handy.computer/docs/paste-methods) · [Report #1038](https://github.com/cjpais/Handy/issues/1038)
- In OpenCode v1 on Windows, a Wispr Flow user reported insertion failing in the VS Code integrated terminal while working in the standalone Windows Terminal. Another user reported OpenWhispr failing with OpenCode in Alacritty while manual paste worked. The issue was closed during the move to OpenCode v2 and contains no v2 retest. [OpenCode report #34499](https://github.com/anomalyco/opencode/issues/34499)
- Classic Outlook is a useful delayed-paste case. OpenWhispr's old-transcript report for desktop Outlook is closed. Wispr Flow documents holding the new clipboard value for about three seconds in both classic and new Outlook. [OpenWhispr report #118](https://github.com/OpenWhispr/openwhispr/issues/118) · [Wispr Flow Outlook behavior](https://docs.wisprflow.ai/articles/9272331366-use-flow-with-microsoft-outlook)
- Handy's Right Alt focus issue in VS Code was reproduced by the maintainer and fixed in v0.10.1. OpenWhispr's Windows Terminal plus WSL Claude Code modifier issue was fixed in v1.6.6. These are regression cases. [Handy #2319](https://github.com/cjpais/Handy/issues/2319) · [OpenWhispr #438](https://github.com/OpenWhispr/openwhispr/issues/438) · [OpenWhispr v1.6.6](https://github.com/OpenWhispr/openwhispr/releases/tag/v1.6.6)

## Delivery in SmallTalkAI

Handy uses Enigo to send its Windows paste chords and provides multiple delivery modes. OpenWhispr uses a native Windows paste helper. SmallTalkAI uses AutoHotkey v2 to type with `SendText` or send a paste chord, selected per executable. Clipboard delivery checks that the intended window and transcript are still current before sending. It can verify insertion in supported standard Edit controls and retains the transcript for manual recovery when it cannot confirm insertion. [Handy Windows input](https://github.com/cjpais/Handy/blob/v0.10.1/src-tauri/src/input.rs) · [OpenWhispr Windows helper](https://github.com/OpenWhispr/openwhispr/pull/473) · [Delivery profiles](../../ahk/smalltalkai.ahk#L234) · [Windows delivery](../../ahk/lib/Typist.ahk#L193)

Start with Notepad as a control. Compare a short phrase and a 200–500 character transcription in Windows Terminal and the VS Code integrated terminal, with the same terminal application running in both hosts. Record the exact versions, delivery mode, target field, whether manual paste succeeds, and whether the clipboard is restored. The source review did not run these comparisons.

## Retired background dictation experiment

The experiment was removed after testing. On 2026-11-24, a Windows test recorded speech, switched to a second window, and verified the exact transcript in the original native Rich Edit field for both typing and Smart Paste. A field that acknowledged edit queries but ignored input produced a failure notice in both modes. The second window retained focus and its original text.

A Chromium test field ignored background typing and paste. The native Read Along test confirmed that page-turn input went only to the saved window, and reader tests confirmed that an unchanged OCR result eventually stops the run. The isolated desktop picker did not open during the combined Read Along check. To verify the complete flow, select the test page on the regular desktop and leave a second window focused while the saved page ignores a turn.
