#Requires AutoHotkey v2.0
#SingleInstance Force

#Include ..\lib\Utils.ahk
#Include ..\lib\TabProtocol.ahk
#Include ..\lib\Logging.ahk
#Include ..\lib\IpcClient.ahk

Fail(message, exitCode := 1) {
    SmallTalkAIConsoleError("FAIL: " . message)
    ExitApp(exitCode)
}

Assert(condition, message) {
    if !condition
        Fail(message)
}

class TestLogger {
    Write(*) {
    }
}

global CallbackCount := 0
global LastResponse := ""

HandleResponse(response) {
    global CallbackCount, LastResponse
    CallbackCount += 1
    LastResponse := response
}

client := IpcClient("missing-helper.exe", A_Temp . "\missing-state.json", "token", (*) => 0, TestLogger())
SetTimer(client.pollEventsTimer, 0)
missingPath := SmallTalkAITempFile("missing-response-race")
try FileDelete(missingPath)
client.jobs[999999] := Map("path", missingPath, "callback", HandleResponse, "kind", "command", "started", A_TickCount, "missing_since", 0)

client.PollJobs()
Assert(CallbackCount = 0, "missing helper response failed immediately instead of waiting for filesystem grace")
Sleep(320)
client.PollJobs()
Assert(CallbackCount = 1, "missing helper response did not fail after grace period")
Assert(!LastResponse["ok"] && InStr(LastResponse["message"], "helper did not create a response file"), "unexpected missing response error")

SmallTalkAIConsoleLine("PASS: IpcClient missing-response grace smoke")
ExitApp(0)
