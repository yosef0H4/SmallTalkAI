//! Dependency-light contracts shared by the controller and disposable workers.
//! Desktop binaries can opt into shared audio-device routing. The base
//! contracts have no audio, OCR, HTTP client, or model runtime dependencies.
pub mod config;
pub mod cuda;
pub mod generation;
pub mod language;
pub mod local_ipc;
pub mod native_paths;
pub mod paddle;
pub mod processing;
pub mod provider_prompts;
pub mod provider_url;
pub mod recorded_action;
pub mod request_budget;
pub mod residency;
pub mod secrets;
pub mod text;
pub mod text_prep;
#[cfg(windows)]
pub mod windows_job;
pub mod wire;

pub use config::{
    AppConfig, ConfigStore, DevicePreference, OcrProvider, SelectionCopyOverride,
    SelectionCopyShortcut, SpeechProvider,
};
pub use generation::{GenerationGate, JobKey, WorkerIncarnation};

pub mod routing;

#[cfg(feature = "desktop-audio")]
pub mod audio_devices {
    //! Match device discovery and default routing for playback and recording.

    use cpal::traits::DeviceTrait;

    pub fn default_host() -> cpal::Host {
        // PipeWire's PulseAudio service exposes virtual microphones that the
        // native CPAL PipeWire backend currently skips.
        #[cfg(target_os = "linux")]
        if let Ok(host) = cpal::host_from_id(cpal::HostId::PulseAudio) {
            return host;
        }
        cpal::default_host()
    }

    pub fn picker_id(device: &cpal::Device) -> Option<String> {
        let id = device.id().ok()?;
        #[cfg(target_os = "linux")]
        match id.host() {
            // These inputs capture speaker output. The dictation picker lists
            // microphones and virtual microphone sources.
            cpal::HostId::PulseAudio if id.id().ends_with(".monitor") => return None,
            cpal::HostId::PipeWire
                if matches!(id.id(), "sink_default" | "input_default" | "output_default") =>
            {
                return None;
            }
            _ => {}
        }
        Some(id.to_string())
    }
}
pub mod provider_error;
