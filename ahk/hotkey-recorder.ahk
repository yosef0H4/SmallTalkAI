#Requires AutoHotkey v2.0
#SingleInstance Force

#Include lib\Utils.ahk
#Include lib\Hotkeys.ahk

if A_Args.Length < 1 || A_Args.Length > 2
    ExitApp 2

outputPath := A_Args[1]
allowCapsLock := A_Args.Length = 2 && A_Args[2] = "--allow-caps-lock"
modifiers := ["CapsLock", "LCtrl", "RCtrl", "LAlt", "RAlt", "LShift", "RShift", "LWin", "RWin"]
modifierNames := Map()
for modifier in modifiers
    modifierNames[GetKeyVK(modifier)] := modifier
held := Map()
capsChord := ""
input := InputHook("L0")
input.KeyOpt("{All}", "N")
input.KeyOpt("{CapsLock}", "S")
input.OnKeyDown := CaptureKeyDown
input.OnKeyUp := CaptureKeyUp
input.Start()
input.Wait()
ExitApp 1

CaptureKeyDown(hook, vk, sc) {
    global modifierNames, modifiers, held, capsChord
    key := GetKeyName(Format("vk{:02X}sc{:03X}", vk, sc))
    if key = "Escape"
        ExitApp 1
    if capsChord != "" || key = ""
        return
    if modifierNames.Has(vk) {
        held[modifierNames[vk]] := true
        return
    }
    if held.Count = 0
        FinishCapture(StrUpper(key), 4)
    ; Read the ordered hook events before stopping the hook, which suppresses Caps Lock.
    prefix := ""
    for modifier in modifiers {
        if held.Has(modifier)
            prefix .= modifier . "+"
    }
    label := prefix . StrUpper(key)
    try HotkeySpec.Parse(label)
    catch
        ExitApp 3
    if held.Has("CapsLock")
        capsChord := label
    else
        FinishCapture(label)
}

CaptureKeyUp(hook, vk, sc) {
    global modifierNames, held, capsChord, allowCapsLock
    if !modifierNames.Has(vk)
        return
    modifier := modifierNames[vk]
    if !held.Has(modifier)
        return
    held.Delete(modifier)
    if modifier = "CapsLock" && capsChord != ""
        FinishCapture(capsChord)
    if held.Count = 0 {
        if modifier = "CapsLock" && allowCapsLock
            FinishCapture("CapsLock")
        FinishCapture(modifier = "CapsLock" ? "Caps Lock" : modifier, 4)
    }
}

FinishCapture(label, exitCode := 0) {
    global outputPath
    try FileDelete(outputPath)
    FileAppend(label, outputPath, "UTF-8")
    ExitApp exitCode
}
