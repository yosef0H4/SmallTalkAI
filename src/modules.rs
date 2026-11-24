//! Installed module inventory and action-time requirements; no native libraries are loaded.
use crate::config::{AppConfig, ModelSelectionMode};
use reader_core::routing;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
fn stt_runtime_path(runtime: &routing::Runtime) -> Option<PathBuf> {
    runtime.asset.as_deref().map(crate::assets::directory)
}

pub fn catalog_stt_runtime_path(path: &Path) -> bool {
    routing::registry()
        .runtimes
        .iter()
        .filter(|runtime| runtime.feature == routing::Feature::Stt)
        .filter_map(stt_runtime_path)
        .any(|candidate| candidate == path)
}
pub fn stt_runtime(config: &AppConfig, language: &str) -> Option<PathBuf> {
    let language_runtime = config
        .speech
        .language_runtimes
        .get(language)
        .and_then(Option::as_ref);
    if let (ModelSelectionMode::FollowKeyboard, Some(id)) =
        (config.speech.selection_mode, language_runtime)
    {
        let runtime = routing::registry()
            .runtimes
            .into_iter()
            .find(|runtime| runtime.id == *id && runtime.feature == routing::Feature::Stt)?;
        return stt_runtime_path(&runtime);
    }
    if config.speech.runtime_dir.is_empty() {
        return routing::bundled_default(routing::Feature::Stt)
            .as_ref()
            .and_then(stt_runtime_path);
    }
    Some(config.parakeet_runtime_dir_path())
}

pub fn choose_runtime(id: &str) -> anyhow::Result<bool> {
    use anyhow::Context;
    let registry = routing::registry();
    let runtime = registry
        .runtimes
        .iter()
        .find(|runtime| runtime.id == id)
        .context("Unknown runtime")?;
    let root = crate::assets::root();
    anyhow::ensure!(
        routing::runtime_installed(runtime, &root),
        "Runtime is not installed"
    );
    if runtime.backend == "cuda" {
        let assets = crate::assets::catalog();
        anyhow::ensure!(
            assets
                .iter()
                .any(|asset| asset.id == crate::assets::NVIDIA_RUNTIME
                    && crate::assets::installed_at(&root, asset)),
            "Install the shared NVIDIA runtime first"
        );
    }
    if runtime.feature == routing::Feature::Stt {
        let path = stt_runtime_path(runtime).context("Runtime has no directory")?;
        let _lock = crate::config::lock_config()?;
        let mut config = AppConfig::load()?;
        config.speech.runtime_dir = path.to_string_lossy().into_owned();
        for language in config.speech.language_models.keys() {
            config
                .speech
                .language_runtimes
                .insert(language.clone(), Some(id.into()));
        }
        config.save()?;
        return Ok(true);
    }
    let mut config = crate::reader::load()?;
    let selection = match runtime.feature {
        routing::Feature::Tts => &mut config.speech.selection,
        routing::Feature::Ocr => &mut config.ocr.selection,
        routing::Feature::Stt => unreachable!(),
    };
    let bindings = std::iter::once(&mut selection.single).chain(selection.languages.values_mut());
    for binding in bindings {
        if let Some(model) = registry
            .models
            .iter()
            .find(|model| Some(&model.id) == binding.model.as_ref())
        {
            anyhow::ensure!(
                model.engine == runtime.engine,
                "Model and runtime use different engines"
            );
        }
        if binding.runtime.as_deref() == Some(id) {
            continue;
        }
        binding.runtime = Some(id.into());
        binding.device = if runtime.backend == "cpu" {
            routing::Device::Cpu
        } else {
            routing::Device::Auto
        };
        binding.webgpu_adapter = None;
    }
    let revision = config.revision;
    crate::reader::store().save(config, revision)?;
    Ok(false)
}
pub fn runtime_ready(path: Option<&std::path::Path>) -> bool {
    path.is_some_and(|path| {
        crate::config::parakeet_native_library_candidates(path)
            .iter()
            .any(|p| p.is_file())
    })
}
pub fn requirement_message(
    feature: &str,
    runtime: bool,
    model: bool,
    language: Option<&str>,
) -> String {
    use crate::localization::{current, translate};
    let locale = current();
    let feature = translate(locale, &format!("module.{feature}"), &[]);
    let runtime = translate(
        locale,
        if runtime {
            "module.ready"
        } else {
            "module.missing"
        },
        &[],
    );
    let model = translate(
        locale,
        if model {
            "module.ready"
        } else {
            "module.missing"
        },
        &[],
    );
    translate(
        locale,
        "module.requirements",
        &[
            ("feature", feature.as_str()),
            ("runtime", runtime.as_str()),
            ("model", model.as_str()),
            ("language", language.unwrap_or("")),
        ],
    )
}
// Driver and GPU capability are necessary signals, not proof that CUDA will load.
fn nvidia_cuda12_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(detect_nvidia_cuda12)
}

// A 12.8-era driver handles PTX on both platforms. Linux includes native
// Pascal kernels, which also passed with the older 550 driver on the GTX 1070.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
fn cuda12_compatible_devices(output: &str, windows: bool) -> bool {
    let minimum_driver = if windows { (570, 65, 0) } else { (570, 26, 0) };
    output.lines().any(|line| {
        let Some((driver, capability)) = line.split_once(',') else {
            return false;
        };
        let mut driver = driver.trim().split('.').map(str::parse::<u32>);
        let version = match (driver.next(), driver.next(), driver.next().transpose()) {
            (Some(Ok(major)), Some(Ok(minor)), Ok(patch)) => (major, minor, patch.unwrap_or(0)),
            _ => return false,
        };
        let Some((major, minor)) = capability.trim().split_once('.') else {
            return false;
        };
        let (Ok(major), Ok(minor)) = (major.parse::<u32>(), minor.parse::<u32>()) else {
            return false;
        };
        let capability = (major, minor);
        let tested_pascal_driver = !windows && capability == (6, 1) && version >= (550, 163, 1);
        capability >= (6, 1) && (version >= minimum_driver || tested_pascal_driver)
    })
}
fn detect_nvidia_cuda12() -> bool {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        use std::io::Read;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        let mut command = Command::new("nvidia-smi");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }
        command
            .args([
                "--query-gpu=driver_version,compute_cap",
                "--format=csv,noheader,nounits",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()
            .is_some_and(|mut child| {
                let deadline = Instant::now() + Duration::from_millis(500);
                loop {
                    match child.try_wait() {
                        Ok(Some(status)) if status.success() => {
                            let mut output = String::new();
                            break child.stdout.take().is_some_and(|out| {
                                out.take(65536).read_to_string(&mut output).is_ok()
                                    && cuda12_compatible_devices(&output, cfg!(windows))
                            });
                        }
                        Ok(None) if Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        _ => {
                            let _ = child.kill();
                            let _ = child.wait();
                            break false;
                        }
                    }
                }
            })
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        false
    }
}
fn runtime_recommendation(id: &str, nvidia_cuda12: bool) -> Option<&'static str> {
    match id {
        "parakeet-vulkan" => Some("vulkan_cpu_gpu"),
        "sherpa-webgpu" | "paddle-webgpu" => Some("webgpu"),
        "sherpa-cuda" if nvidia_cuda12 => Some("nvidia_driver_detected"),
        "sherpa-cpu" if !nvidia_cuda12 => Some("cpu_voice"),
        "paddle-cpu" => Some("small_capture_cpu"),
        _ => None,
    }
}

fn model_recommendation(id: &str, usable_nvidia_voice: bool) -> Option<&'static str> {
    match id {
        "kokoro" if usable_nvidia_voice => Some("nvidia_voice"),
        "supertonic3" if usable_nvidia_voice => Some("multilingual_voice"),
        "supertonic3" => Some("cpu_voice"),
        "paddle-ar-en" => Some("available_ocr"),
        _ => None,
    }
}

pub fn inventory(config: &AppConfig) -> Value {
    let registry = routing::registry();
    let reader = crate::reader::load().ok();
    let nvidia_cuda12 = nvidia_cuda12_available();
    let usable_nvidia_voice = nvidia_cuda12
        && registry.runtimes.iter().any(|runtime| {
            runtime.id == "sherpa-cuda"
                && routing::runtime_installed(runtime, &crate::assets::root())
        });
    let runtimes=registry.runtimes.iter().map(|runtime| {
        let directory=runtime.asset.as_ref().map(|id|crate::assets::directory(id));
        let installed=reader_core::routing::runtime_installed(runtime,&crate::assets::root());
        let bundled=runtime.asset.as_deref().is_some_and(|id|routing::bundled_asset(id).is_some());
        let default = bundled && matches!(runtime.id.as_str(), "parakeet-vulkan" | "sherpa-cpu" | "paddle-cpu");
        let selected = match runtime.feature {
            routing::Feature::Stt => directory.as_ref().is_some_and(|path| Some(path) == stt_runtime(config, "").as_ref()),
            routing::Feature::Tts => reader.as_ref().and_then(|config| config.speech.selection.single.runtime.as_deref()) == Some(runtime.id.as_str()),
            routing::Feature::Ocr => reader.as_ref().and_then(|config| config.ocr.selection.single.runtime.as_deref()) == Some(runtime.id.as_str()),
        };
        let action_asset = if runtime.backend == "cuda" { Some(crate::assets::NVIDIA_RUNTIME) } else { runtime.asset.as_deref() };
        let recommendation = runtime_recommendation(&runtime.id, nvidia_cuda12);
        json!({"id":runtime.id,"feature":runtime.feature,"engine":runtime.engine,"backend":runtime.backend,"label":runtime.label,"installed":installed,"bundled":bundled,"default":default,"selected":selected,"directory":directory,"asset":runtime.asset,"action_asset":action_asset,"recommended":recommendation.is_some(),"recommendation":recommendation})
    }).collect::<Vec<_>>();
    let mut models=registry.models.iter().map(|model| {
        let recommendation = model_recommendation(&model.id, usable_nvidia_voice);
        json!({"id":model.id,"feature":model.feature,"engine":model.engine,"label":model.label,"languages":model.languages,"installed":routing::asset_installed(&model.asset,&crate::assets::root()),"asset":model.asset,"recommended":recommendation.is_some(),"recommendation":recommendation})
    }).collect::<Vec<_>>();
    models.extend(crate::models::catalog_for_config(config).iter().map(|model| {
        let languages=model.languages.iter().filter_map(|label|label.rsplit_once('(').map(|(_,id)|id.trim_end_matches(')').to_owned())).collect::<Vec<_>>();
        json!({"id":model.file,"feature":"stt","engine":"parakeet","label":format!("{} · {}",model.family,model.quant),"languages":languages,"installed":model.installed,"recommended":model.recommended,"recommendation":model.recommended.then_some("speech_model"),"asset":null,"size_mb":model.size_mb,"benchmark_wer":crate::models::benchmark_wer(&model.file)})
    }));
    json!({"runtimes":runtimes,"models":models})
}

/// Immutable snapshot used by completion-only automatic assignment.
#[derive(Clone)]
pub struct AutoInventory {
    pub registry: routing::Registry,
    pub installed: std::collections::BTreeSet<String>,
    pub stt_models: Vec<crate::models::ModelSpec>,
    pub nvidia_cuda12: bool,
}
impl AutoInventory {
    pub fn capture(config: &AppConfig) -> Self {
        let registry = routing::registry();
        let mut installed = std::collections::BTreeSet::new();
        for asset in crate::assets::catalog() {
            if crate::assets::installed_at(&crate::assets::root(), &asset) {
                installed.insert(asset.id);
            }
        }
        Self {
            registry,
            installed,
            stt_models: crate::models::installed_models(config),
            nvidia_cuda12: nvidia_cuda12_available(),
        }
    }
    fn has_runtime(&self, runtime: &routing::Runtime) -> bool {
        runtime.asset.as_ref().map_or_else(
            || self.installed.contains(&runtime.id),
            |asset| self.installed.contains(asset),
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoChanges {
    pub stt: bool,
    pub reader: bool,
}

/// Apply only after a user-requested download completed successfully.
pub fn auto_select(
    stt: &mut AppConfig,
    reader: &mut reader_core::AppConfig,
    completed: &str,
    languages: &[String],
    inventory: &AutoInventory,
) -> AutoChanges {
    let mut result = AutoChanges::default();
    let speech_event = inventory
        .stt_models
        .iter()
        .any(|model| model.file == completed)
        || inventory.registry.runtimes.iter().any(|runtime| {
            runtime.feature == routing::Feature::Stt
                && (runtime.asset.as_deref() == Some(completed)
                    || (runtime.asset.is_none() && runtime.id == completed))
        });
    if speech_event {
        result.stt = auto_select_stt(stt, completed, languages, inventory);
    }
    for feature in [routing::Feature::Tts, routing::Feature::Ocr] {
        let relevant = inventory.registry.runtimes.iter().any(|runtime| {
            runtime.feature == feature && runtime.asset.as_deref() == Some(completed)
        }) || inventory
            .registry
            .models
            .iter()
            .any(|model| model.feature == feature && model.asset == completed);
        if !relevant {
            continue;
        }
        let selection = if feature == routing::Feature::Tts {
            &mut reader.speech.selection
        } else {
            &mut reader.ocr.selection
        };
        result.reader |= auto_select_reader(selection, feature, completed, languages, inventory);
    }
    result
}

/// Merge completion selections into the latest on-disk configurations.
pub fn persist_auto_select(completed: &str) -> anyhow::Result<AutoChanges> {
    let _guard = crate::config::lock_config()?;
    let mut stt = AppConfig::load()?;
    let languages = crate::capture::input_language::keyboard_languages()
        .languages
        .into_iter()
        .map(|language| language.id)
        .collect::<Vec<_>>();
    let inventory = AutoInventory::capture(&stt);
    let mut reader = crate::reader::load()?;
    let changes = auto_select(&mut stt, &mut reader, completed, &languages, &inventory);
    if changes.stt {
        stt.save()?;
    }
    if changes.reader {
        for attempt in 0..3 {
            let revision = reader.revision;
            match crate::reader::store().save(reader.clone(), revision) {
                Ok(_) => return Ok(changes),
                Err(error) if attempt < 2 && error.to_string().contains("revision conflict") => {
                    reader = crate::reader::load()?;
                    let mut latest_stt = stt.clone();
                    if !auto_select(
                        &mut latest_stt,
                        &mut reader,
                        completed,
                        &languages,
                        &inventory,
                    )
                    .reader
                    {
                        return Ok(AutoChanges {
                            stt: changes.stt,
                            reader: false,
                        });
                    }
                }
                Err(error) => return Err(error),
            }
        }
    }
    Ok(changes)
}

fn auto_select_stt(
    config: &mut AppConfig,
    completed: &str,
    languages: &[String],
    inventory: &AutoInventory,
) -> bool {
    let mut runtimes: Vec<_> = inventory
        .registry
        .runtimes
        .iter()
        .filter(|r| r.feature == routing::Feature::Stt && inventory.has_runtime(r))
        .collect();
    runtimes.sort_by_key(|r| {
        (
            r.asset.as_deref() != Some(completed),
            runtime_recommendation(&r.id, inventory.nvidia_cuda12).is_none(),
        )
    });
    let mut models: Vec<_> = inventory
        .stt_models
        .iter()
        .filter(|m| m.installed && !m.languages.is_empty())
        .collect();
    models.sort_by_key(|m| (m.file != completed, !m.recommended));
    let mut changed = false;
    if config.speech.selection_mode == ModelSelectionMode::SingleModel {
        let Some(runtime) = runtimes.iter().find(|runtime| {
            config.speech.runtime_dir.is_empty()
                || stt_runtime_path(runtime)
                    .is_some_and(|path| config.parakeet_runtime_dir_path() == path)
        }) else {
            return false;
        };
        let model = models.iter().find(|model| {
            config
                .speech
                .single_model_filename
                .as_ref()
                .is_none_or(|existing| existing == &model.file)
        });
        if let Some(model) = model {
            if config.speech.single_model_filename.is_none() {
                config.speech.single_model_filename = Some(model.file.clone());
                changed = true;
            }
            if config.speech.runtime_dir.is_empty() {
                if let Some(path) = stt_runtime_path(runtime) {
                    config.speech.runtime_dir = path.to_string_lossy().into_owned();
                    changed = true;
                }
            }
        }
    } else {
        for language in languages {
            let runtime = runtimes.iter().find(|runtime| {
                config
                    .speech
                    .language_runtimes
                    .get(language)
                    .and_then(Option::as_ref)
                    .is_none_or(|existing| existing == &runtime.id)
            });
            let model = models.iter().find(|model| {
                config
                    .speech
                    .language_models
                    .get(language)
                    .and_then(Option::as_ref)
                    .is_none_or(|existing| existing == &model.file)
                    && model.languages.iter().any(|label| {
                        label
                            .rsplit_once('(')
                            .is_some_and(|(_, id)| id.trim_end_matches(')') == language)
                    })
            });
            if let (Some(runtime), Some(model)) = (runtime, model) {
                if config
                    .speech
                    .language_models
                    .get(language)
                    .is_none_or(Option::is_none)
                {
                    config
                        .speech
                        .language_models
                        .insert(language.clone(), Some(model.file.clone()));
                    changed = true;
                }
                if config
                    .speech
                    .language_runtimes
                    .get(language)
                    .is_none_or(Option::is_none)
                {
                    config
                        .speech
                        .language_runtimes
                        .insert(language.clone(), Some(runtime.id.clone()));
                    changed = true;
                }
            }
        }
    }
    changed
}

fn auto_select_reader(
    selection: &mut routing::Selection,
    feature: routing::Feature,
    completed: &str,
    languages: &[String],
    inventory: &AutoInventory,
) -> bool {
    let mut changed = false;
    match selection.mode {
        routing::SelectionMode::SingleModel => {
            changed |=
                fill_reader_binding(&mut selection.single, feature, None, completed, inventory)
        }
        routing::SelectionMode::FollowKeyboard | routing::SelectionMode::AutoDetect => {
            let mut targets: std::collections::BTreeSet<String> =
                languages.iter().map(|l| routing::primary(l)).collect();
            targets.extend(selection.languages.keys().cloned());
            for language in targets {
                let mut binding = selection
                    .languages
                    .get(&language)
                    .cloned()
                    .unwrap_or_default();
                if fill_reader_binding(&mut binding, feature, Some(&language), completed, inventory)
                {
                    selection.languages.insert(language, binding);
                    changed = true;
                }
            }
        }
    }
    changed
}

fn fill_reader_binding(
    binding: &mut routing::Binding,
    feature: routing::Feature,
    language: Option<&str>,
    completed: &str,
    inventory: &AutoInventory,
) -> bool {
    let mut runtimes: Vec<_> = inventory
        .registry
        .runtimes
        .iter()
        .filter(|runtime| {
            runtime.feature == feature
                && inventory.has_runtime(runtime)
                && binding.runtime.as_ref().is_none_or(|id| id == &runtime.id)
                && (binding.device != routing::Device::Gpu || runtime.backend != "cpu")
        })
        .collect();
    runtimes.sort_by_key(|runtime| {
        (
            runtime.asset.as_deref() != Some(completed),
            runtime_recommendation(&runtime.id, inventory.nvidia_cuda12).is_none(),
        )
    });
    let mut models: Vec<_> = inventory
        .registry
        .models
        .iter()
        .filter(|model| {
            model.feature == feature
                && inventory.installed.contains(&model.asset)
                && binding.model.as_ref().is_none_or(|id| id == &model.id)
                && language.is_none_or(|language| {
                    model.languages.is_empty() || model.languages.iter().any(|l| l == language)
                })
        })
        .collect();
    let nvidia_installed = inventory.nvidia_cuda12
        && inventory
            .registry
            .runtimes
            .iter()
            .any(|r| r.id == "sherpa-cuda" && inventory.has_runtime(r));
    models.sort_by_key(|model| {
        (
            model.asset != completed,
            model_recommendation(&model.id, nvidia_installed).is_none(),
        )
    });
    for runtime in runtimes {
        for model in &models {
            if runtime.engine != model.engine {
                continue;
            }
            let mut candidate = binding.clone();
            if candidate.runtime.is_none() {
                candidate.runtime = Some(runtime.id.clone());
            }
            if candidate.model.is_none() {
                candidate.model = Some(model.id.clone());
            }
            let test = routing::Selection {
                single: candidate.clone(),
                ..Default::default()
            };
            if test.validate(feature).is_ok() && candidate != *binding {
                *binding = candidate;
                return true;
            }
        }
    }
    false
}

pub fn stt_model_ready(config: &AppConfig, language: &str) -> bool {
    let Some(path) = config.model_path_for(language).filter(|p| p.is_file()) else {
        return false;
    };
    if config.speech.selection_mode == ModelSelectionMode::SingleModel {
        return true;
    }
    crate::models::catalog_for_config(config)
        .iter()
        .find(|model| {
            path.file_name()
                .is_some_and(|name| name == model.file.as_str())
        })
        .is_none_or(|model| {
            model.languages.is_empty()
                || model.languages.iter().any(|label| {
                    label
                        .rsplit_once('(')
                        .is_some_and(|(_, id)| id.trim_end_matches(')') == language)
                })
        })
}

#[cfg(test)]
mod recommendation_tests {
    use super::*;

    const VULKAN_ASSET: &str = if cfg!(windows) {
        "parakeet-vulkan-windows"
    } else {
        "parakeet-vulkan"
    };

    fn inventory_fixture() -> AutoInventory {
        let mut stt_models = crate::models::catalog();
        for model in &mut stt_models {
            model.installed = model.file == "tdt_ctc-110m-q8_0.gguf"
                || model.file == "lemura-arabic-asr-lite-q8_0.gguf";
        }
        AutoInventory {
            registry: routing::registry(),
            installed: [
                VULKAN_ASSET,
                if cfg!(windows) {
                    "sherpa-cpu-windows"
                } else {
                    "sherpa-cpu"
                },
                "kokoro",
                "supertonic3",
                "paddle-ar-en",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            stt_models,
            nvidia_cuda12: false,
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_cuda_language_binding_resolves_the_catalog_runtime() {
        let mut config = AppConfig::default();
        config
            .speech
            .language_runtimes
            .insert("en".into(), Some("parakeet-cuda".into()));
        let resolved = stt_runtime(&config, "en").unwrap();
        assert_eq!(resolved, crate::assets::directory("parakeet-cuda-windows"));
        assert_eq!(stt_runtime(&config, "ar"), None);
    }
    #[test]
    fn completion_assigns_a_runtime_for_single_model_mode() {
        let mut stt = AppConfig::default();
        stt.speech.selection_mode = ModelSelectionMode::SingleModel;
        let mut reader = reader_core::AppConfig::default();
        assert!(
            auto_select(
                &mut stt,
                &mut reader,
                VULKAN_ASSET,
                &[],
                &inventory_fixture()
            )
            .stt
        );
        assert!(!stt.speech.runtime_dir.is_empty());
    }
    #[test]
    fn completion_fills_only_compatible_empty_speech_bindings() {
        let inventory = inventory_fixture();
        let mut stt = AppConfig::default();
        let mut reader = reader_core::AppConfig::default();
        let languages = vec!["en".into(), "ar".into()];
        let changed = auto_select(&mut stt, &mut reader, VULKAN_ASSET, &languages, &inventory);
        assert!(changed.stt);
        assert_eq!(
            stt.speech.language_models["en"].as_deref(),
            Some("tdt_ctc-110m-q8_0.gguf")
        );
        assert_eq!(
            stt.speech.language_models["ar"].as_deref(),
            Some("lemura-arabic-asr-lite-q8_0.gguf")
        );
        stt.speech
            .language_models
            .insert("en".into(), Some("custom.gguf".into()));
        let before = stt.clone();
        auto_select(
            &mut stt,
            &mut reader,
            "tdt_ctc-110m-q8_0.gguf",
            &languages,
            &inventory,
        );
        assert_eq!(stt, before);
    }

    #[test]
    fn completion_preserves_reader_choices_and_checks_language_and_device() {
        let inventory = inventory_fixture();
        let mut stt = AppConfig::default();
        let mut reader = reader_core::AppConfig::default();
        reader
            .speech
            .selection
            .languages
            .entry("en".into())
            .or_default()
            .voice = Some("af_bella".into());
        let changed = auto_select(
            &mut stt,
            &mut reader,
            "kokoro",
            &["ar".into(), "en".into()],
            &inventory,
        );
        assert!(changed.reader);
        assert_eq!(
            reader.speech.selection.languages["en"].model.as_deref(),
            Some("kokoro")
        );
        assert_eq!(
            reader.speech.selection.languages["en"].voice.as_deref(),
            Some("af_bella")
        );
        assert_eq!(
            reader.speech.selection.languages["ar"].model.as_deref(),
            Some("supertonic3")
        );
        assert_eq!(reader.speech.selection.languages.len(), 2);
        let before = reader.clone();
        auto_select(&mut stt, &mut reader, "unrelated", &[], &inventory);
        assert_eq!(reader, before);
        let mut binding = routing::Binding {
            device: routing::Device::Gpu,
            ..Default::default()
        };
        assert!(!fill_reader_binding(
            &mut binding,
            routing::Feature::Tts,
            Some("en"),
            "kokoro",
            &inventory
        ));
    }

    #[test]
    fn runtime_policy_recommends_vulkan_dictation_regardless_of_cuda_detection() {
        for nvidia_cuda12 in [false, true] {
            assert_eq!(
                runtime_recommendation("parakeet-vulkan", nvidia_cuda12),
                Some("vulkan_cpu_gpu")
            );
            assert_eq!(runtime_recommendation("parakeet-cuda", nvidia_cuda12), None);
        }
    }

    #[test]
    fn reader_runtime_policy_preserves_cpu_and_cuda_recommendations() {
        assert_eq!(
            runtime_recommendation("sherpa-cuda", true),
            Some("nvidia_driver_detected")
        );
        assert_eq!(
            runtime_recommendation("sherpa-cpu", false),
            Some("cpu_voice")
        );
        assert_eq!(runtime_recommendation("sherpa-cpu", true), None);
        assert_eq!(
            runtime_recommendation("paddle-cpu", true),
            Some("small_capture_cpu")
        );
        assert_eq!(runtime_recommendation("paddle-cuda", true), None);
    }

    #[test]
    fn cuda12_recognizes_pascal_and_platform_driver_versions() {
        let linux = false;
        let windows = true;
        assert!(cuda12_compatible_devices("550.163.01, 6.1\n", linux));
        assert!(cuda12_compatible_devices("572.16, 12.0\n", windows));
        assert!(!cuda12_compatible_devices("550.163.00, 8.9\n", linux));
        assert!(!cuda12_compatible_devices("550.163.01, 8.9\n", linux));
        assert!(!cuda12_compatible_devices("570.64, 8.9\n", windows));
        assert!(!cuda12_compatible_devices("590.01, 5.2\n", windows));
        assert!(!cuda12_compatible_devices("590.01.01, [N/A]\n", linux));
        assert!(!cuda12_compatible_devices("unknown, 8.9\n", linux));
        assert!(cuda12_compatible_devices(
            "570.26.00, 5.2\n570.26.00, 8.0\n",
            linux
        ));
    }
    #[test]
    fn voice_recommendations_depend_on_installed_nvidia_runtime() {
        assert_eq!(
            model_recommendation("supertonic3", false),
            Some("cpu_voice")
        );
        assert_eq!(model_recommendation("kokoro", false), None);
        assert_eq!(model_recommendation("kokoro", true), Some("nvidia_voice"));
        assert_eq!(
            model_recommendation("supertonic3", true),
            Some("multilingual_voice")
        );
        assert_eq!(model_recommendation("unknown", true), None);
    }
}
