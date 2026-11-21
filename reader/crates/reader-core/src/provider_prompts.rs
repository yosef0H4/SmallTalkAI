//! Prompt defaults and compatibility rules shared by Settings and the workers.

use serde::Deserialize;
use std::sync::LazyLock;

pub const SPEECH_ENV: &str = "SIMPLE_TTS_REMOTE_SPEECH_INSTRUCTIONS";
pub const OCR_ENV: &str = "SIMPLE_TTS_REMOTE_OCR_PROMPT";

#[derive(Deserialize)]
struct Defaults<'a> {
    stt: &'a str,
    tts: &'a str,
    ocr: &'a str,
}

static DEFAULTS: LazyLock<Defaults<'static>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../web/settings/src/lib/provider-prompts.json"
    ))
    .expect("embedded provider prompt defaults")
});

pub fn validate(prompt: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        prompt.chars().count() <= 4096 && !prompt.contains('\0'),
        "Provider instructions must contain at most 4096 characters and no NUL bytes"
    );
    Ok(())
}

pub fn transcription<'a>(model: &str, prompt: &'a str) -> Option<&'a str> {
    if model.starts_with("gpt-4o-transcribe-diarize") {
        return None;
    }
    let prompt = prompt.trim();
    if !prompt.is_empty() {
        return Some(prompt);
    }
    (model.starts_with("gpt-4o-transcribe") || model.starts_with("gpt-4o-mini-transcribe"))
        .then_some(DEFAULTS.stt)
}

pub fn speech<'a>(model: &str, instructions: &'a str) -> Option<&'a str> {
    if matches!(model, "tts-1" | "tts-1-hd") {
        return None;
    }
    let instructions = instructions.trim();
    if !instructions.is_empty() {
        return Some(instructions);
    }
    model.starts_with("gpt-4o-mini-tts").then_some(DEFAULTS.tts)
}

pub fn ocr(prompt: &str) -> &str {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        DEFAULTS.ocr
    } else {
        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_prompts_and_legacy_models_remain_compatible() {
        assert_eq!(transcription("whisper-1", ""), None);
        assert_eq!(transcription("gpt-4o-transcribe-diarize", "Names"), None);
        assert_eq!(
            transcription("gpt-4o-mini-transcribe", ""),
            Some(DEFAULTS.stt)
        );
        assert_eq!(transcription("whisper-1", "  Names  "), Some("Names"));
        assert_eq!(speech("tts-1-hd", "Speak slowly"), None);
        assert_eq!(speech("gpt-4o-mini-tts", ""), Some(DEFAULTS.tts));
        assert_eq!(speech("kokoro-v1", ""), None);
        assert_eq!(speech("custom-tts", " Speak slowly "), Some("Speak slowly"));
        assert_eq!(ocr("  "), DEFAULTS.ocr);
        assert_eq!(ocr("  Return text only. "), "Return text only.");
        assert!(validate("bad\0prompt").is_err());
    }
}
