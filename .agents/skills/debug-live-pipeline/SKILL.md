# Skill: debug the SmallTalkAI pipeline

Use the component boundaries in order:

```powershell
.\scripts\test-static.ps1
cargo test --all-targets
.\scripts\check-prereqs.ps1 -RequireRuntime
.\scripts\run-dev.ps1 -SkipBuild
```

Expected live sequence:

```text
AHK hotkey down
→ smalltalkai-ctl start-recording
→ capture retains 20 ms PCM frames
→ AHK hotkey up
→ smalltalkai-ctl stop-recording
→ capture launches or reuses smalltalkai-infer
→ worker loads model lazily when cold
→ worker returns Unicode transcript
→ AHK event poll queues focus-checked SendText chunks
→ idle timeout terminates smalltalkai-infer process
```

Inspect separate logs under `%LOCALAPPDATA%\smalltalkai\logs`. Never “fix” capture failures by loading Parakeet inside `smalltalkai-capture.exe`.
