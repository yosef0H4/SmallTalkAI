SmallTalkAIQuote(value) {
    return Chr(34) . value . Chr(34)
}

SmallTalkAIResolveExe(stem) {
    sameDir := A_ScriptDir . "\" . stem . ".exe"
    if FileExist(sameDir)
        return sameDir
    parentDir := A_ScriptDir . "\..\" . stem . ".exe"
    if FileExist(parentDir)
        return parentDir
    release := A_ScriptDir . "\..\target\release\" . stem . ".exe"
    if FileExist(release)
        return release
    releaseFromParent := A_ScriptDir . "\..\..\target\release\" . stem . ".exe"
    if FileExist(releaseFromParent)
        return releaseFromParent
    debug := A_ScriptDir . "\..\target\debug\" . stem . ".exe"
    if FileExist(debug)
        return debug
    debugFromParent := A_ScriptDir . "\..\..\target\debug\" . stem . ".exe"
    if FileExist(debugFromParent)
        return debugFromParent
    return sameDir
}

SmallTalkAITempFile(prefix := "smalltalkai") {
    dir := A_Temp . "\smalltalkai-shell"
    DirCreate(dir)
    return dir . "\" . prefix . "-" . A_TickCount . "-" . Random(100000, 999999) . ".txt"
}

SmallTalkAIRandomToken() {
    bytes := Buffer(32, 0)
    ok := DllCall("advapi32\SystemFunction036", "Ptr", bytes.Ptr, "UInt", bytes.Size, "Int")
    token := ""
    if ok {
        Loop bytes.Size
            token .= Format("{:02x}", NumGet(bytes, A_Index - 1, "UChar"))
        return token
    }
    Loop 8
        token .= Format("{:08x}", Random(0, 0x7fffffff))
    return token
}

SmallTalkAIBool(value) {
    return value = true || value = 1 || StrLower(value . "") = "true"
}

SmallTalkAIBoolText(value) {
    return SmallTalkAIBool(value) ? "true" : "false"
}

SmallTalkAINoticeEndsSession(event) {
    return event.Has("values")
        && IsObject(event["values"])
        && event["values"].Has("terminal")
        && SmallTalkAIBool(event["values"]["terminal"])
}

SmallTalkAINextDeliveryMode(current, configured := "smart_paste,type") {
    modes := []
    for mode in StrSplit(configured, ",") {
        mode := Trim(mode)
        if mode != ""
            modes.Push(mode)
    }
    if !modes.Length
        modes := ["smart_paste", "type"]
    for index, mode in modes {
        if mode = current
            return modes[Mod(index, modes.Length) + 1]
    }
    return modes[1]
}

SmallTalkAIWriteStdOut(text := "") {
    try FileAppend(text, "*", "UTF-8")
}

SmallTalkAIWriteStdErr(text := "") {
    try FileAppend(text, "**", "UTF-8")
}

SmallTalkAIConsoleLine(text := "") {
    SmallTalkAIWriteStdOut(text . "`n")
}

SmallTalkAIConsoleError(text) {
    SmallTalkAIWriteStdErr(text . "`n")
}
