#Requires AutoHotkey v2.0
#SingleInstance Force

#Include lib\Utils.ahk
#Include lib\Hotkeys.ahk

if A_Args.Length < 1 || A_Args.Length > 2
    ExitApp 2

outputPath := A_Args[1]
allowCapsLock := A_Args.Length = 2 && A_Args[2] = "--allow-caps-lock"
input := InputHook("L1")
input.KeyOpt("{All}", "E")
input.KeyOpt("{LCtrl}{RCtrl}{LAlt}{RAlt}{LShift}{RShift}{LWin}{RWin}{CapsLock}", "-E")
; Capture CapsLock in this input hook so holding it cannot flood the hotkey dispatcher.
input.KeyOpt("{CapsLock}", "S")
if allowCapsLock {
    input.KeyOpt("{CapsLock}", "N")
    input.OnKeyUp := CaptureCapsUp
}
input.Start()
input.Wait()

key := input.EndKey
if key = "" || key = "Escape"
    ExitApp 1

parts := Array()
for modifier in ["CapsLock", "LCtrl", "RCtrl", "LAlt", "RAlt", "LShift", "RShift", "LWin", "RWin"] {
    if GetKeyState(modifier, "P")
        parts.Push(modifier)
}
parts.Push(StrUpper(key))
label := ""
for index, part in parts
    label .= (index > 1 ? "+" : "") . part

try HotkeySpec.Parse(label)
catch
    ExitApp 3

try FileDelete(outputPath)
FileAppend(label, outputPath, "UTF-8")
ExitApp 0

CaptureCapsUp(hook, *) {
    global outputPath
    if hook.InProgress {
        try FileDelete(outputPath)
        FileAppend("CapsLock", outputPath, "UTF-8")
        ExitApp 0
    }
}
