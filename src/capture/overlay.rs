#[path = "overlay_model.rs"]
pub mod overlay_model;

#[cfg(target_os = "linux")]
#[path = "overlay_font.rs"]
pub mod overlay_font;

#[cfg(target_os = "linux")]
#[path = "overlay_render.rs"]
pub mod overlay_render;

#[cfg(windows)]
#[path = "overlay_windows.rs"]
mod platform;

#[cfg(target_os = "linux")]
#[path = "overlay_linux.rs"]
mod platform;

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::overlay_model::{OverlayPrimary, RecordingIndicators};
    use anyhow::Result;
    use std::sync::{atomic::AtomicU32, Arc};
    use std::time::Duration;
    #[derive(Debug, Clone)]
    pub struct OverlayHandle {
        waveform: Arc<AtomicU32>,
    }
    impl OverlayHandle {
        pub fn spawn() -> Result<Self> {
            Ok(Self {
                waveform: Arc::new(AtomicU32::new(0)),
            })
        }
        pub fn start_recording(&self, _: isize, _: RecordingIndicators) {}
        pub fn set_recording_transcript(&self, _: usize, _: &str) {}
        pub fn set_primary(&self, _: OverlayPrimary) {}
        pub fn set_locale(&self, _: crate::localization::Locale) {}
        pub fn set_shortcuts(&self, _: String, _: bool) {}
        pub fn notify_info(&self, _: impl Into<String>, _: Option<Duration>) {}
        pub fn notify_warning(&self, _: impl Into<String>, _: Duration) {}
        pub fn notify_error(&self, _: impl Into<String>, _: Duration) {}
        pub fn clear_notice(&self) {}
        pub fn waveform_cell(&self) -> Arc<AtomicU32> {
            Arc::clone(&self.waveform)
        }
        pub fn hide(&self) {}
    }
}
pub use overlay_model::{
    ascii_visualizer, empty_visualizer_levels, render_overlay_text, set_visualizer_level,
    waveform_for_level, waveform_text, NoticeLevel, OverlayPrimary, RecordingIndicators,
};
pub use platform::OverlayHandle;

impl OverlayHandle {
    pub fn set_level(&self, level: f32) {
        self.waveform_cell().store(
            overlay_model::pack_waveform(level),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

#[cfg(any(windows, target_os = "linux"))]
impl OverlayHandle {
    pub fn start_recording(&self, _target_window: isize, indicators: RecordingIndicators) {
        self.send(overlay_model::OverlayCommand::StartRecording(indicators));
    }

    pub fn set_recording_transcript(&self, number: usize, text: &str) {
        self.send(overlay_model::OverlayCommand::RecordingTranscript(
            number,
            overlay_model::recording_excerpt(text),
        ));
    }

    pub fn set_shortcuts(&self, text: String, enabled: bool) {
        self.send(overlay_model::OverlayCommand::SetShortcuts(text, enabled));
    }

    pub fn set_locale(&self, locale: crate::localization::Locale) {
        self.send(overlay_model::OverlayCommand::SetLocale(locale));
    }

    pub fn set_primary(&self, primary: OverlayPrimary) {
        self.send(overlay_model::OverlayCommand::SetPrimary(primary));
    }

    pub fn notify_info(&self, text: impl Into<String>, duration: Option<std::time::Duration>) {
        self.notify(NoticeLevel::Info, text, duration);
    }

    pub fn notify_warning(&self, text: impl Into<String>, duration: std::time::Duration) {
        self.notify(NoticeLevel::Warning, text, Some(duration));
    }

    pub fn notify_error(&self, text: impl Into<String>, duration: std::time::Duration) {
        self.notify(NoticeLevel::Error, text, Some(duration));
    }

    pub fn clear_notice(&self) {
        self.send(overlay_model::OverlayCommand::ClearNotice);
    }

    pub fn hide(&self) {
        self.send(overlay_model::OverlayCommand::Hide);
    }

    fn notify(
        &self,
        level: NoticeLevel,
        text: impl Into<String>,
        duration: Option<std::time::Duration>,
    ) {
        self.send(overlay_model::OverlayCommand::Notify {
            level,
            text: text.into(),
            duration,
        });
    }
}
