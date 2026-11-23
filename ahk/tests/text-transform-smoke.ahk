#Requires AutoHotkey v2.0
#SingleInstance Force

#Include ..\lib\TextTransform.ahk

Fail(message, exitCode := 1) {
    FileAppend("FAIL: " . message . "`n", "*")
    ExitApp(exitCode)
}

sample := "Hello, WORLD! #1"
if SmallTalkAITransformTranscript(sample, false, false) != sample
    Fail("identity transform changed text")
if SmallTalkAITransformTranscript(sample, true, false) != "Hello WORLD 1"
    Fail("punctuation transform mismatch")
if SmallTalkAITransformTranscript(sample, false, true) != "hello, world! #1"
    Fail("lowercase transform mismatch")
if SmallTalkAITransformTranscript(sample, true, true) != "hello world 1"
    Fail("combined transform mismatch")

FileAppend("PASS: text-transform smoke`n", "*")
ExitApp(0)
