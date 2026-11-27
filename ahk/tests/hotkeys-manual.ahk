#Requires AutoHotkey v2.0
#SingleInstance Force

#Include ..\lib\Utils.ahk
#Include ..\lib\Logging.ahk
#Include ..\lib\Hotkeys.ahk

global DownCount := 0
global UpCount := 0

Down() {
    global DownCount
    DownCount += 1
}

Up() {
    global UpCount
    UpCount += 1
}

Fail(message, exitCode := 1) {
    SmallTalkAIConsoleError("FAIL: " . message)
    ExitApp(exitCode)
}

logger := ShellLog(A_Temp . "\smalltalkai-hotkeys-manual.log")
capsController := CapsLockTapController(logger)
manager := HotkeyManager(Down, Up, logger, capsController)
manager.Configure("CapsLock+S", true, "preserve_tap")
if !manager.enabled
    Fail("manager should be enabled after Configure()")
cancelManager := HotkeyManager(Down, Up, logger, capsController)
cancelManager.Configure("CapsLock+A", true, "preserve_tap")
toggleManager := HotkeyManager(Down, Up, logger, capsController)
toggleManager.Configure("CapsLock+D", true, "preserve_tap")
helpManager := HotkeyManager(Down, Up, logger, capsController)
helpManager.Configure("CapsLock+H", true, "preserve_tap")
if capsController.count != 4
    Fail("shared CapsLock controller should include the shortcut reminder binding")

for label in ["CapsLock+S", "LCtrl+S", "AltGr+S", "RAlt+S", "LShift+X"] {
    spec := HotkeySpec.Parse(label)
    if !spec.Has("down") || !spec.Has("up")
        Fail("parsed hotkey spec is incomplete for " . label)
}
for label in ["CapsLock", "Ctrl", "Ctrl+Shift"] {
    try {
        HotkeySpec.Parse(label)
        Fail("a normal shortcut accepted modifier-only input: " . label)
    } catch {
        continue
    }
}

manager.Configure("LCtrl+S", true, "preserve_tap")
manager.Configure("LCtrl+S", true, "preserve_tap", false)
if manager.releaseStops
    Fail("toggle recording mode should not stop on physical key release")
manager.Configure("LCtrl+S", true, "preserve_tap", true)
if !manager.releaseStops
    Fail("hold recording mode should stop on physical key release")
manager.SetEnabled(false)
if manager.enabled
    Fail("manager should be disabled after SetEnabled(false)")
manager.SetEnabled(true)
if !manager.enabled
    Fail("manager should be enabled after SetEnabled(true)")
manager.Configure("None", true, "preserve_tap")
if manager.enabled
    Fail("None should leave a hotkey disabled")
if !HotkeySpec.IsNone("disabled") || !HotkeySpec.IsNone("off")
    Fail("disabled aliases should be recognized as None")
manager.Configure("AltGr+S", true, "preserve_tap")
manager.DisableBindings()
cancelManager.DisableBindings()
toggleManager.DisableBindings()
helpManager.DisableBindings()
if capsController.count != 0
    Fail("shared CapsLock controller should unregister all bindings")

releaseManager := HotkeyManager(Down, Up, logger, capsController)
releaseManager.Configure("CapsLock+F24", true, "preserve_tap")
capsController.HandleCapsDown()
beforeDown := DownCount
beforeUp := UpCount
releaseManager.WaitForKeyRelease()
releaseManager.HandleUp()
if DownCount != beforeDown + 1 || UpCount != beforeUp + 1
    Fail("physical release should end a chord once even when its up hotkey is missing")
capsController.Consume()
capsController.HandleCapsUp()
releaseManager.DisableBindings()

capsController.ConfigureHint("Ctrl+F24", "preserve_tap", "shortcut-expiry-test")
capsController.ShowHint()
hintHwnd := capsController.hintWindow.Hwnd
hintLease := DllCall("GetPropW", "ptr", hintHwnd, "str", "SmallTalkAIShortcutHold", "ptr")
tick := DllCall("GetTickCount64", "uint64")
if hintLease <= tick || hintLease > tick + 600
    Fail("shortcut hint must publish a bounded deadline")
capsController.RefreshHint()
if DllCall("GetPropW", "ptr", hintHwnd, "str", "SmallTalkAIShortcutHold", "ptr")
    Fail("shortcut hint should clear if the key was released without an up callback")
capsController.ConfigureHint("None", "preserve_tap", "")

screenshotNotice := Map(
    "kind", "notice",
    "session_id", 42,
    "text", "Screen context requested",
    "values", Map()
)
if SmallTalkAINoticeEndsSession(screenshotNotice)
    Fail("screen-context progress must preserve the recording session")
terminalNotice := Map(
    "kind", "notice",
    "session_id", 42,
    "text", "No speech detected",
    "values", Map("terminal", "true")
)
if !SmallTalkAINoticeEndsSession(terminalNotice)
    Fail("terminal recording notices must close the recording session")

cycle := "smart_paste,paste_ctrl_shift_v,clipboard"
rules := "exact`tType`tkitty.exe`ncontains`tpaste_ctrl_shift_v`tkitty"
if SmallTalkAIPasteModeForApp(rules, "KITTY.EXE") != "type"
    throw Error("Smart Paste exact application rule did not take priority")
if SmallTalkAIPasteModeForApp(rules, "org.kitty") != "paste_ctrl_shift_v"
    throw Error("Smart Paste substring rule did not match")
if SmallTalkAIPasteModeForApp("", "Kitty.exe") != "paste_ctrl_v"
    throw Error("Smart Paste did not fall back to Ctrl+V after removing its rules")
if SmallTalkAINextDeliveryMode("smart_paste", cycle) != "paste_ctrl_shift_v"
    Fail("delivery cycle should include advanced paste modes")
if SmallTalkAINextDeliveryMode("paste_ctrl_shift_v", cycle) != "clipboard"
    Fail("delivery cycle should preserve configured order")
if SmallTalkAINextDeliveryMode("clipboard", cycle) != "smart_paste"
    Fail("delivery cycle should wrap")
if SmallTalkAINextDeliveryMode("type", cycle) != "smart_paste"
    Fail("delivery cycle should start at the first choice when current mode is outside the cycle")

SmallTalkAIConsoleLine("PASS: hotkey parser and binding smoke")
ExitApp(0)
