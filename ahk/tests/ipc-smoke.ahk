#Requires AutoHotkey v2.0
#SingleInstance Force

#Include ..\lib\Utils.ahk
#Include ..\lib\TabProtocol.ahk
#Include ..\lib\Config.ahk

Fail(message, exitCode := 1) {
    SmallTalkAIConsoleError("FAIL: " . message)
    ExitApp(exitCode)
}

Info(message) {
    SmallTalkAIConsoleLine("INFO: " . message)
}

ctl := SmallTalkAIResolveExe("smalltalkai-ctl")
capture := SmallTalkAIResolveExe("smalltalkai-capture")
if !FileExist(ctl) || !FileExist(capture) {
    Fail("Build smalltalkai-ctl.exe and smalltalkai-capture.exe first.")
}
configFile := SmallTalkAITempFile("ipc-smoke-config")
state := SmallTalkAITempFile("ipc-smoke-state")
pid := 0
EnvSet("SMALLTALKAI_CONFIG", configFile)
OnExit(Cleanup)
OnError(ReportError)
config := ConfigStore(ctl)
token := SmallTalkAIRandomToken()
try FileDelete(state)
Info("starting capture service")
Run(SmallTalkAIQuote(capture) . " --token " . SmallTalkAIQuote(token) . " --state-file " . SmallTalkAIQuote(state) . " --config " . SmallTalkAIQuote(config.Get("config_path")), A_ScriptDir, "Hide", &pid)
Loop 40 {
    Sleep(100)
    if FileExist(state)
        break
}
if !FileExist(state)
    Fail("capture state file was not published")
output := SmallTalkAITempFile("ipc-smoke")
RunWait(SmallTalkAIQuote(ctl) . " --state-file " . SmallTalkAIQuote(state) . " --token " . SmallTalkAIQuote(token) . " --output " . SmallTalkAIQuote(output) . " ping", A_ScriptDir, "Hide")
response := TabProtocol.ReadResponse(output)
try FileDelete(output)
if !response["ok"] {
    try ProcessClose(pid)
    Fail("PING failed: " . response["message"])
}
if response["message"] != "pong" {
    ProcessClose(pid)
    Fail("unexpected ping response: " . response["message"])
}
output := SmallTalkAITempFile("ipc-shutdown")
RunWait(SmallTalkAIQuote(ctl) . " --state-file " . SmallTalkAIQuote(state) . " --token " . SmallTalkAIQuote(token) . " --output " . SmallTalkAIQuote(output) . " shutdown", A_ScriptDir, "Hide")
try FileDelete(output)
try ProcessWaitClose(pid, 3)
SmallTalkAIConsoleLine("PASS: authenticated ping and graceful shutdown")
ExitApp(0)

ReportError(err, mode) {
    SmallTalkAIConsoleError("FAIL: " . err.Message)
    ExitApp(2)
    return true
}
Cleanup(*) {
    global pid, state, configFile
    if pid && ProcessExist(pid)
        try ProcessClose(pid)
    try FileDelete(state)
    try FileDelete(configFile)
}
