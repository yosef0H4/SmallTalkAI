use crate::localization::{translate, Locale};
use std::time::{Duration, Instant};

pub const BAR_COUNT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPrimary {
    Hidden,
    Recording,
    Transcribing,
    Typing,
    Cleaning,
    PreparingClipboard,
    Pasting,
    WaitingForPaste,
    WaitingForKeys,
}

impl OverlayPrimary {
    pub fn for_delivery(phase: crate::common::shell_protocol::DeliveryPhase) -> Self {
        use crate::common::shell_protocol::DeliveryPhase;
        match phase {
            DeliveryPhase::Typing => Self::Typing,
            DeliveryPhase::WaitingForKeys => Self::WaitingForKeys,
            DeliveryPhase::PreparingClipboard => Self::PreparingClipboard,
            DeliveryPhase::Pasting => Self::Pasting,
            DeliveryPhase::WaitingForPaste => Self::WaitingForPaste,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecordingIndicators {
    pub ai_cleanup: bool,
    pub screen_context: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum NoticeLevel {
    #[default]
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub(super) enum OverlayCommand {
    SetLocale(Locale),
    #[cfg(target_os = "linux")]
    SetTheme(crate::config::UiTheme),
    #[cfg(target_os = "linux")]
    SystemDark(bool),
    SetShortcuts(String, bool),
    StartRecording(RecordingIndicators),
    RecordingTranscript(usize, String),
    SetPrimary(OverlayPrimary),
    Notify {
        level: NoticeLevel,
        text: String,
        duration: Option<Duration>,
    },
    ClearNotice,
    Hide,
}

impl OverlayCommand {
    pub(super) fn resets_waveform(&self) -> bool {
        match self {
            Self::StartRecording(_) | Self::Hide => true,
            Self::SetPrimary(primary) => *primary != OverlayPrimary::Recording,
            _ => false,
        }
    }
}

struct Notice {
    level: NoticeLevel,
    text: String,
    expires_at: Option<Instant>,
}

pub(super) struct OverlayContent {
    pub(super) primary: OverlayPrimary,
    pub(super) locale: Locale,
    #[cfg(target_os = "linux")]
    theme: crate::config::UiTheme,
    #[cfg(target_os = "linux")]
    system_dark: bool,
    shortcuts: String,
    shortcut_reminders: bool,
    indicators: RecordingIndicators,
    recording_transcript: Option<(usize, String)>,
    notice: Option<Notice>,
}

impl Default for OverlayContent {
    fn default() -> Self {
        Self {
            primary: OverlayPrimary::Hidden,
            locale: Locale::En,
            #[cfg(target_os = "linux")]
            theme: crate::config::UiTheme::default(),
            #[cfg(target_os = "linux")]
            system_dark: false,
            shortcuts: String::new(),
            shortcut_reminders: true,
            indicators: RecordingIndicators::default(),
            recording_transcript: None,
            notice: None,
        }
    }
}

impl OverlayContent {
    pub(super) fn apply(&mut self, command: OverlayCommand, now: Instant) {
        match command {
            #[cfg(target_os = "linux")]
            OverlayCommand::SetTheme(theme) => self.theme = theme,
            #[cfg(target_os = "linux")]
            OverlayCommand::SystemDark(dark) => self.system_dark = dark,
            OverlayCommand::SetLocale(locale) => {
                if self.locale != locale {
                    self.locale = locale;
                    self.notice = None;
                }
            }
            OverlayCommand::SetShortcuts(text, enabled) => {
                self.shortcuts = text;
                self.shortcut_reminders = enabled;
            }
            OverlayCommand::StartRecording(indicators) => {
                self.primary = OverlayPrimary::Recording;
                self.indicators = indicators;
                self.notice = None;
                self.recording_transcript = None;
            }
            OverlayCommand::RecordingTranscript(number, text) => {
                if self.primary == OverlayPrimary::Recording {
                    self.recording_transcript = Some((number, text));
                }
            }
            OverlayCommand::SetPrimary(primary) => {
                self.primary = primary;
                if primary != OverlayPrimary::Recording {
                    self.recording_transcript = None;
                }
            }
            OverlayCommand::Notify {
                level,
                text,
                duration,
            } => {
                self.expire_notice(now);
                let text = text.trim();
                // Repeated callbacks must not extend a notice indefinitely or
                // replace an active warning with a lower-priority message.
                if !text.is_empty()
                    && !self.notice.as_ref().is_some_and(|current| {
                        current.level > level || (current.level == level && current.text == text)
                    })
                {
                    self.notice = Some(Notice {
                        level,
                        text: text.to_owned(),
                        expires_at: duration.map(|duration| now + duration),
                    });
                }
            }
            OverlayCommand::ClearNotice => self.notice = None,
            OverlayCommand::Hide => {
                self.primary = OverlayPrimary::Hidden;
                self.notice = None;
                self.recording_transcript = None;
            }
        }
    }

    pub(super) fn expire_notice(&mut self, now: Instant) -> bool {
        let expired = self
            .notice
            .as_ref()
            .and_then(|notice| notice.expires_at)
            .is_some_and(|deadline| now >= deadline);
        if expired {
            self.notice = None;
        }
        expired
    }

    #[cfg(target_os = "linux")]
    pub(super) fn dark_theme(&self) -> bool {
        use crate::config::UiTheme;
        self.theme == UiTheme::Dark || (self.theme == UiTheme::Auto && self.system_dark)
    }

    #[cfg(windows)]
    pub(super) fn has_status(&self) -> bool {
        self.primary != OverlayPrimary::Hidden || self.notice.is_some()
    }

    #[cfg(windows)]
    pub(super) fn has_shortcuts(&self) -> bool {
        !self.shortcuts.is_empty()
    }

    pub(super) fn text(&self, levels: &VisualizerLevels, show_shortcuts: bool) -> String {
        let mut content = render_overlay_text_with_locale(
            self.primary,
            self.notice.as_ref().map(|notice| notice.text.as_str()),
            levels,
            self.indicators,
            self.locale,
        );
        if let Some((number, text)) = &self.recording_transcript {
            content.push('\n');
            content.push_str(&translate(
                self.locale,
                "notice.recordingChunk",
                &[("number", &number.to_string())],
            ));
            if !text.is_empty() {
                content.push_str(" · \u{2068}");
                content.push_str(text);
                content.push('\u{2069}');
            }
        }
        if show_shortcuts && content.is_empty() {
            self.shortcuts.clone()
        } else {
            crate::shortcuts::append(
                content,
                &self.shortcuts,
                self.shortcut_reminders || show_shortcuts,
            )
        }
    }
}

/// Keep the most recent words visible without growing the overlay with the recording.
pub(super) fn recording_excerpt(text: &str) -> String {
    const MAX_CHARACTERS: usize = 140;
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let Some((index, previous)) = text.char_indices().rev().nth(MAX_CHARACTERS) else {
        return text;
    };
    let tail = &text[index + previous.len_utf8()..];
    let words = if previous == ' ' {
        tail
    } else {
        tail.split_once(' ').map_or(tail, |(_, words)| words)
    };
    format!("…{words}")
}

pub type VisualizerLevels = [f32; BAR_COUNT];

pub fn empty_visualizer_levels() -> VisualizerLevels {
    [0.0; BAR_COUNT]
}

pub fn set_visualizer_level(levels: &mut VisualizerLevels, level: f32) {
    let level = level.clamp(0.0, 1.0);
    for (idx, slot) in levels.iter_mut().enumerate() {
        let from_edge = idx.min(BAR_COUNT - 1 - idx);
        let distance = 1.0 - from_edge as f32 / (BAR_COUNT / 2 - 1) as f32;
        *slot = (level.powf(1.0 + 1.2 * distance) * (1.35 - 0.35 * distance)).min(1.0);
    }
}

// Ten three-bit bar heights fit in one atomic update from the audio callback.
pub fn pack_waveform(level: f32) -> u32 {
    let mut levels = empty_visualizer_levels();
    set_visualizer_level(&mut levels, level);
    levels.iter().enumerate().fold(0, |frame, (i, &height)| {
        frame | (bar_index(height) as u32) << (i * 3)
    })
}

pub(super) fn unpack_waveform(frame: u32) -> VisualizerLevels {
    std::array::from_fn(|i| ((frame >> (i * 3)) & 7) as f32 / (WAVEFORM_GLYPHS.len() - 1) as f32)
}

pub fn waveform_text(frame: u32) -> String {
    ascii_visualizer(&unpack_waveform(frame))
}

pub fn waveform_for_level(level: f32) -> String {
    let mut levels = empty_visualizer_levels();
    set_visualizer_level(&mut levels, level);
    ascii_visualizer(&levels)
}

const WAVEFORM_GLYPHS: &[char] = &[
    '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
];

fn bar_index(level: f32) -> usize {
    let strength = (level + 0.04).clamp(0.0, 1.0);
    (strength * (WAVEFORM_GLYPHS.len() - 1) as f32).round() as usize
}

pub fn ascii_visualizer(levels: &VisualizerLevels) -> String {
    let mut line = String::with_capacity(BAR_COUNT * 3);
    line.extend(
        levels
            .iter()
            .map(|&level| WAVEFORM_GLYPHS[bar_index(level)]),
    );
    line
}

pub fn render_overlay_text(
    primary: OverlayPrimary,
    notice_text: Option<&str>,
    visualizer: &VisualizerLevels,
    indicators: RecordingIndicators,
) -> String {
    render_overlay_text_with_locale(
        primary,
        notice_text,
        visualizer,
        indicators,
        crate::localization::Locale::En,
    )
}

pub fn render_overlay_text_with_locale(
    primary: OverlayPrimary,
    notice_text: Option<&str>,
    visualizer: &VisualizerLevels,
    indicators: RecordingIndicators,
    locale: crate::localization::Locale,
) -> String {
    let recording = primary == OverlayPrimary::Recording;
    let primary = match primary {
        OverlayPrimary::Hidden => None,
        OverlayPrimary::Recording => {
            let mut line = String::from("🎙");
            if indicators.ai_cleanup {
                line.push_str(" AI");
                if indicators.screen_context {
                    line.push(' ');
                    line.push_str(&crate::localization::translate(
                        locale,
                        "overlay.screenContext",
                        &[],
                    ));
                }
            }
            line.push(' ');
            line.push_str(&ascii_visualizer(visualizer));
            Some(if locale == crate::localization::Locale::Ar {
                format!("\u{2066}{line}\u{2069}")
            } else {
                line
            })
        }
        OverlayPrimary::Transcribing => Some(crate::localization::translate(
            locale,
            "overlay.transcribing",
            &[],
        )),
        OverlayPrimary::Cleaning => Some(crate::localization::translate(
            locale,
            "overlay.cleaning",
            &[],
        )),
        OverlayPrimary::PreparingClipboard => Some(crate::localization::translate(
            locale,
            "overlay.preparingClipboard",
            &[],
        )),
        OverlayPrimary::Pasting => Some(crate::localization::translate(
            locale,
            "overlay.pasting",
            &[],
        )),
        OverlayPrimary::WaitingForPaste => Some(crate::localization::translate(
            locale,
            "overlay.waitingForPaste",
            &[],
        )),
        OverlayPrimary::WaitingForKeys => Some(crate::localization::translate(
            locale,
            "overlay.waitingForKeys",
            &[],
        )),
        OverlayPrimary::Typing => Some(crate::localization::translate(
            locale,
            "overlay.typing",
            &[],
        )),
    };
    let primary = primary.map(|text| {
        // Keep the microphone marker on the live waveform.
        if recording {
            text
        } else {
            compact_notice(&text, locale)
        }
    });
    let notice = notice_text.map(|text| compact_notice(text, locale));
    match (primary, notice.filter(|text| !text.is_empty())) {
        (Some(primary), Some(notice)) => format!("{primary}\n{notice}"),
        (Some(primary), None) => primary,
        (None, Some(notice)) => notice,
        (None, None) => String::new(),
    }
}

fn is_shortcut_row(text: &str) -> bool {
    text.contains(" \u{2066}[") && text.ends_with("]\u{2069}")
}

/// Use the same text for status and reading progress in both presenters.
fn compact_notice(text: &str, locale: Locale) -> String {
    let mut result = String::with_capacity(text.len());
    for line in text.trim().lines() {
        let line = line.trim();
        let body = if is_shortcut_row(line) {
            line
        } else if let Some((prefix, rest)) = line.split_once(' ') {
            match prefix.trim_end_matches('\u{fe0f}') {
                "⏸" if rest.starts_with('\u{2068}') => {
                    result.push_str(&crate::localization::translate(
                        locale,
                        "reader.phase.paused",
                        &[],
                    ));
                    result.push_str(" · ");
                    rest
                }
                "🎙" | "🔊" | "📋" | "📄" | "🌐" | "🔍" | "⏸" | "⏳" | "⟳" | "⚠" => {
                    rest
                }
                _ => line,
            }
        } else {
            line
        };
        if is_shortcut_row(body) {
            result.push_str(body);
        } else {
            result.extend(body.chars().map(|ch| if ch == '\t' { '\n' } else { ch }));
        }
        result.push('\n');
    }
    result.trim_end().to_owned()
}

// Common Controls needs explicit LTR embeddings and spaces for shortcut rows.
// Keep Pango's Unicode isolates and tabs in the shared text.
#[cfg(any(windows, test))]
pub(super) fn windows_tooltip_text(text: &str, locale: Locale) -> String {
    let mut result = String::with_capacity(text.len());
    for line in text.lines() {
        let rtl_shortcuts = locale == Locale::Ar && is_shortcut_row(line);
        if rtl_shortcuts {
            result.push('\u{202a}');
        }
        for ch in line.chars() {
            match ch {
                '\u{2066}'..='\u{2069}' | '\u{fe0f}' => (),
                '\t' => result.push_str("   "),
                _ => result.push(ch),
            }
        }
        if rtl_shortcuts {
            result.push('\u{202c}');
        }
        result.push('\n');
    }
    result.trim_end().to_owned()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_excerpt_sits_below_waveform_and_clears_at_stop() {
        let mut content = OverlayContent {
            locale: Locale::En,
            ..Default::default()
        };
        let now = Instant::now();
        let levels = empty_visualizer_levels();
        content.apply(OverlayCommand::StartRecording(Default::default()), now);
        content.apply(
            OverlayCommand::RecordingTranscript(1, recording_excerpt("first\nمرحبا")),
            now,
        );
        let visible = content.text(&levels, false);
        assert!(visible.starts_with("🎙"));
        assert!(visible.ends_with("\nChunk 1 · \u{2068}first مرحبا\u{2069}"));
        let next = format!("{}last words", "earlier words ".repeat(20));
        content.apply(
            OverlayCommand::RecordingTranscript(2, recording_excerpt(&next)),
            now,
        );
        let visible = content.text(&levels, false);
        assert!(visible.contains("\nChunk 2 · \u{2068}…"));
        assert!(visible.ends_with("last words\u{2069}"));
        assert!(!visible.contains("Chunk 1"));
        content.apply(
            OverlayCommand::SetPrimary(OverlayPrimary::Transcribing),
            now,
        );
        assert!(!content.text(&levels, false).contains("Chunk 2"));
        content.apply(OverlayCommand::StartRecording(Default::default()), now);
        assert!(!content.text(&levels, false).contains("Chunk 2"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn explicit_themes_win_over_system_preference() {
        use crate::config::UiTheme;
        let mut content = OverlayContent::default();
        let now = Instant::now();
        for dark in [false, true] {
            content.apply(OverlayCommand::SystemDark(dark), now);
            for (theme, expected) in [
                (UiTheme::Light, false),
                (UiTheme::Dark, true),
                (UiTheme::Auto, dark),
            ] {
                content.apply(OverlayCommand::SetTheme(theme), now);
                assert_eq!(content.dark_theme(), expected);
            }
        }
    }
    #[test]
    fn both_presentations_share_every_delivery_phase() {
        use crate::common::shell_protocol::DeliveryPhase;
        for (phase, expected, key) in [
            (
                DeliveryPhase::PreparingClipboard,
                OverlayPrimary::PreparingClipboard,
                "overlay.preparingClipboard",
            ),
            (
                DeliveryPhase::Pasting,
                OverlayPrimary::Pasting,
                "overlay.pasting",
            ),
            (
                DeliveryPhase::WaitingForPaste,
                OverlayPrimary::WaitingForPaste,
                "overlay.waitingForPaste",
            ),
            (
                DeliveryPhase::WaitingForKeys,
                OverlayPrimary::WaitingForKeys,
                "overlay.waitingForKeys",
            ),
            (
                DeliveryPhase::Typing,
                OverlayPrimary::Typing,
                "overlay.typing",
            ),
        ] {
            let primary = OverlayPrimary::for_delivery(phase);
            assert_eq!(primary, expected);
            for locale in [Locale::En, Locale::Ar] {
                let text = render_overlay_text_with_locale(
                    primary,
                    None,
                    &empty_visualizer_levels(),
                    RecordingIndicators::default(),
                    locale,
                );
                let expected = crate::localization::translate(locale, key, &[]);
                assert_eq!(text, expected.trim_start_matches("🎙 "));
                assert_eq!(windows_tooltip_text(&text, locale), text);
            }
        }
    }

    #[test]
    fn presenters_share_reading_text_and_keep_shaping_controls_where_supported() {
        let sample = "⏸️ \u{2068}A passage.\u{2069} · 2/6\r\n📄 Next page · ready";
        let mut overlay = OverlayContent::default();
        overlay.apply(
            OverlayCommand::Notify {
                level: NoticeLevel::Info,
                text: sample.into(),
                duration: None,
            },
            Instant::now(),
        );
        let text = overlay.text(&empty_visualizer_levels(), false);
        assert_eq!(
            text,
            "Paused · \u{2068}A passage.\u{2069} · 2/6\nNext page · ready"
        );
        assert_eq!(
            windows_tooltip_text(&text, Locale::En),
            "Paused · A passage. · 2/6\nNext page · ready"
        );
        let arabic = compact_notice(
            "🔊 \u{2068}نص \u{2066}SmallTalkAI\u{2069}\u{2069}",
            Locale::Ar,
        );
        assert_eq!(arabic, "\u{2068}نص \u{2066}SmallTalkAI\u{2069}\u{2069}");
        assert_eq!(windows_tooltip_text(&arabic, Locale::Ar), "نص SmallTalkAI");
    }

    #[test]
    fn repeated_notices_expire_and_lower_priority_cannot_hide_an_active_error() {
        let mut overlay = OverlayContent::default();
        let now = Instant::now();
        let levels = empty_visualizer_levels();
        let deadline = Duration::from_secs(5);
        let error = OverlayCommand::Notify {
            level: NoticeLevel::Error,
            text: "⚠️ Reading failed".into(),
            duration: Some(deadline),
        };
        overlay.apply(error.clone(), now);
        overlay.apply(error, now + Duration::from_secs(2));
        overlay.apply(
            OverlayCommand::Notify {
                level: NoticeLevel::Info,
                text: "Next page · ready".into(),
                duration: None,
            },
            now + Duration::from_secs(4),
        );
        assert_eq!(overlay.text(&levels, false), "Reading failed");
        assert!(overlay.expire_notice(now + deadline));
        assert!(overlay.text(&levels, false).is_empty());
    }

    #[test]
    fn native_tooltip_keeps_emoji_shortcuts_in_compact_rows() {
        let bindings = [
            ("record".to_owned(), "CapsLock+S".to_owned()),
            ("cancel".to_owned(), "CapsLock+A".to_owned()),
            ("copy-region".to_owned(), "CapsLock+Z".to_owned()),
        ]
        .into();
        for locale in [Locale::En, Locale::Ar] {
            let shortcuts = crate::shortcuts::summary(&bindings, locale);
            let text = windows_tooltip_text(&shortcuts, locale);
            let rows: Vec<_> = text
                .lines()
                .map(|line| line.trim_matches(['\u{202a}', '\u{202c}']))
                .collect();
            assert_eq!(
                rows,
                ["🔴 [CapsLock][S]   ❌ [CapsLock][A]", "📋 [CapsLock][Z]"]
            );
        }
    }

    #[test]
    fn waveform_uses_audio_level_without_phase_motion() {
        let mut levels = empty_visualizer_levels();
        set_visualizer_level(&mut levels, 0.4);
        let first = ascii_visualizer(&levels);
        let same = ascii_visualizer(&levels);
        set_visualizer_level(&mut levels, 0.8);
        let louder = ascii_visualizer(&levels);
        assert_eq!(first.chars().count(), BAR_COUNT);
        assert_eq!(first, same);
        assert_ne!(first, louder);
        assert!(first.chars().all(|glyph| {
            "\u{2581}\u{2582}\u{2583}\u{2584}\u{2585}\u{2586}\u{2587}".contains(glyph)
        }));
    }

    #[test]
    fn render_combines_primary_and_notice() {
        let text = render_overlay_text(
            OverlayPrimary::Transcribing,
            Some("Loading speech model..."),
            &empty_visualizer_levels(),
            RecordingIndicators::default(),
        );
        assert!(text.contains("Transcribing"));
        assert!(text.contains("Loading speech model..."));
    }

    #[test]
    fn recording_shows_ai_and_screen_privacy_indicators() {
        let text = render_overlay_text(
            OverlayPrimary::Recording,
            None,
            &empty_visualizer_levels(),
            RecordingIndicators {
                ai_cleanup: true,
                screen_context: true,
            },
        );
        assert!(text.starts_with("🎙 AI Screen "));
    }

    #[test]
    fn screen_indicator_never_appears_without_ai() {
        let text = render_overlay_text(
            OverlayPrimary::Recording,
            None,
            &empty_visualizer_levels(),
            RecordingIndicators {
                ai_cleanup: false,
                screen_context: true,
            },
        );
        assert_eq!(
            text,
            format!("🎙 {}", ascii_visualizer(&empty_visualizer_levels()))
        );
    }
}
