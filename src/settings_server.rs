use crate::capture::state::ServiceState;
use crate::cleanup::{self, auth, secrets};
use crate::common::shell_protocol::{
    ClientMessage, ServerMessage, ShellCommand, ShellResponse, SHELL_PROTOCOL_VERSION,
};
use crate::config::AppConfig;
use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const INDEX: &str = include_str!("../web/settings/dist/index.html");
const CSS: &str = include_str!("../web/settings/dist/styles.css");
const ARABIC_FONT: &[u8] = include_bytes!("../web/settings/dist/arabic.woff2");
const SETTINGS_FONT: &[u8] = include_bytes!("../web/settings/dist/settings-sans.woff2");
const SETTINGS_FONT_LICENSE: &str = include_str!("../web/settings/dist/settings-sans-license.txt");
const JS: &str = include_str!("../web/settings/dist/app.js");
const MAX_BODY: usize = 2 * 1024 * 1024;
const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Parser)]
#[command(name = "smalltalkai-settings")]
pub struct Args {
    #[arg(long)]
    state_file: Option<PathBuf>,
    #[arg(long)]
    service_token: Option<String>,
    /// Run the authenticated local Settings API without opening a window.
    #[arg(long, hide = true)]
    headless: bool,
}

#[derive(Clone)]
struct CaptureConnection {
    state_file: PathBuf,
    token: Option<String>,
}

struct AppState {
    assets: crate::assets::Manager,
    web_token: String,
    origin: String,
    capture: Option<CaptureConnection>,
    last_activity: Mutex<Instant>,
    closing: AtomicBool,
    focus_pending: Option<AtomicBool>,
    config_lock: Mutex<()>,
    profile_lock: RwLock<()>,
    cleanup_auth_status: Arc<Mutex<Value>>,
}

#[derive(Deserialize)]
struct SaveRequest {
    config: Value,
    expected_hash: String,
}

#[derive(Default, Serialize, Deserialize)]
struct ModelSetup {
    done: bool,
    step: u8,
    #[serde(default)]
    skip_reader: bool,
    runtimes: [Option<String>; 3],
    models: [Option<String>; 3],
    theme: Option<crate::config::UiTheme>,
    voice: Option<String>,
}

fn setup_path() -> PathBuf {
    AppConfig::config_path().with_file_name("model-setup.json")
}

fn model_setup() -> Result<ModelSetup> {
    let mut draft = match fs::read(setup_path()) {
        Ok(data) => {
            let mut draft: ModelSetup = serde_json::from_slice(&data)?;
            // Step seven was the separate Read aloud runtime picker.
            if draft.step == 7
                && !draft.done
                && draft.runtimes[1].as_deref() != Some("paddle-webgpu")
            {
                draft.step = if draft.runtimes[2].is_some() { 8 } else { 4 };
            }
            draft
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let config = AppConfig::load()?;
            let speech = &config.speech;
            let reader = crate::reader::load()?;
            let done = speech.single_model_filename.is_some()
                || speech.language_models.values().any(Option::is_some)
                || speech.provider != crate::config::SpeechProvider::Local
                || [reader.speech.selection, reader.ocr.selection]
                    .iter()
                    .any(|selection| {
                        selection.single.model.is_some()
                            || selection.single.remote.is_some()
                            || selection
                                .languages
                                .values()
                                .any(|binding| binding.model.is_some() || binding.remote.is_some())
                    });
            ModelSetup {
                done,
                ..Default::default()
            }
        }
        Err(error) => return Err(error.into()),
    };
    if !draft.done {
        use reader_core::routing::{bundled_default, Feature};
        for (value, feature) in
            draft
                .runtimes
                .iter_mut()
                .zip([Feature::Stt, Feature::Ocr, Feature::Tts])
        {
            if value.is_none() {
                *value = bundled_default(feature).map(|runtime| runtime.id);
            }
        }
    }
    Ok(draft)
}

fn save_model_setup(next: ModelSetup, state: &AppState) -> Result<ModelSetup> {
    use crate::admin::Feature;
    let _lock = state.config_lock.lock().expect("config lock");
    let previous = model_setup()?;
    if previous.done {
        return Ok(previous);
    }
    anyhow::ensure!(next.step <= 10, "Invalid model setup step");
    for (index, feature) in [Feature::Stt, Feature::Ocr, Feature::Tts]
        .into_iter()
        .enumerate()
    {
        if next.models[index] != previous.models[index]
            || next.runtimes[index] != previous.runtimes[index]
        {
            if let (Some(model), Some(runtime)) = (&next.models[index], &next.runtimes[index]) {
                crate::admin::select_assets(feature, model, runtime, None, false)?;
                if matches!(feature, Feature::Stt) {
                    let _ = capture_request(state, ShellCommand::ReloadConfig);
                }
            }
        }
    }
    if next.models[2].is_some() && next.voice != previous.voice {
        let store = crate::reader::store();
        let mut reader = crate::reader::load()?;
        let revision = reader.revision;
        reader.speech.selection.single.voice = next.voice.clone();
        store.save(reader, revision)?;
    }
    if let Some(theme) = next.theme.filter(|_| next.theme != previous.theme) {
        let _guard = crate::config::lock_config()?;
        let mut config = AppConfig::load()?;
        config.general.ui_theme = theme;
        config.save()?;
    }
    let path = setup_path();
    let temp = crate::config::unique_atomic_temp_path(&path);
    fs::write(&temp, serde_json::to_vec(&next)?)?;
    crate::config::replace_file_atomic(&temp, &path)?;
    Ok(next)
}

#[derive(Deserialize)]
struct ActionRequest {
    action: String,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    language: crate::config::SpeechLanguage,
}

#[derive(Deserialize)]
struct CleanupActionRequest {
    action: String,
    #[serde(default)]
    config: Value,
    #[serde(default)]
    secret: String,
    #[serde(default)]
    transcript: String,
}

#[derive(Serialize, Deserialize)]
struct SettingsSession {
    instance_id: String,
    pid: u32,
    origin: String,
    token: String,
}

// Keep this file in place: unlinking a locked file would let another launcher
// lock a different inode. The OS releases the lock on normal exit or a crash.
struct SettingsInstance {
    _lock: fs::File,
    session: Option<SettingsSession>,
}

impl Drop for SettingsInstance {
    fn drop(&mut self) {
        if let Some(owned) = &self.session {
            if let Ok(bytes) = fs::read(settings_session_path(&owned.instance_id)) {
                if let Ok(current) = serde_json::from_slice::<SettingsSession>(&bytes) {
                    if current.instance_id == owned.instance_id
                        && current.pid == owned.pid
                        && current.token == owned.token
                    {
                        let _ = fs::remove_file(settings_session_path(&current.instance_id));
                    }
                }
            }
        }
    }
}

enum SettingsLaunch {
    Start(SettingsInstance),
    Reuse(String),
}

impl Args {
    pub fn desktop(&self) -> bool {
        !self.headless
    }
}

pub struct DesktopSettings {
    pub url: url::Url,
    server: Server,
    state: Arc<AppState>,
}

impl DesktopSettings {
    pub fn serve(self, focus: impl Fn() + Send + 'static, close: impl FnOnce() + Send + 'static) {
        std::thread::spawn(move || {
            if let Err(error) = serve_settings(&self.server, &self.state, Some(&focus)) {
                eprintln!("smalltalkai-settings: {error:#}");
            }
            close();
        });
    }
}

pub fn run(args: Args, window: Option<fn(DesktopSettings) -> Result<()>>) -> Result<()> {
    let desktop = window.is_some();
    anyhow::ensure!(desktop == args.desktop(), "invalid Settings launch mode");
    let web_token = random_token();
    let instance_id = if desktop { "desktop" } else { "headless" };
    let mut instance = match settings_launch(instance_id)? {
        SettingsLaunch::Start(instance) => instance,
        SettingsLaunch::Reuse(url) => {
            if desktop {
                focus_desktop(&url)?;
            } else {
                println!("{url}");
            }
            return Ok(());
        }
    };
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let origin = format!("http://{}", listener.local_addr()?);
    let server = Server::from_listener(listener, None)
        .map_err(|error| anyhow::anyhow!("starting settings server: {error}"))?;
    let capture = args
        .state_file
        .or_else(|| desktop.then(AppConfig::service_state_path))
        .map(|state_file| CaptureConnection {
            state_file,
            token: args.service_token,
        });
    let state = Arc::new(AppState {
        assets: crate::assets::Manager::default(),
        web_token: web_token.clone(),
        origin: origin.clone(),
        capture,
        last_activity: Mutex::new(Instant::now()),
        closing: AtomicBool::new(false),
        focus_pending: desktop.then(|| AtomicBool::new(false)),
        config_lock: Mutex::new(()),
        profile_lock: RwLock::new(()),
        cleanup_auth_status: Arc::new(Mutex::new(json!({"state":"idle"}))),
    });
    let weak_state = Arc::downgrade(&state);
    state.assets.set_completion_hook(Arc::new(
        move |id| match crate::modules::persist_auto_select(id) {
            Ok(changes) => {
                if changes.stt {
                    if let Some(state) = weak_state.upgrade() {
                        let _ = capture_request(&state, ShellCommand::ReloadConfig);
                    }
                }
                changes.stt || changes.reader
            }
            Err(error) => {
                eprintln!("automatic selection after download failed: {error:#}");
                false
            }
        },
    ));
    let url = format!("{origin}/#token={web_token}");
    let session = SettingsSession {
        instance_id: instance_id.to_owned(),
        pid: std::process::id(),
        origin,
        token: web_token,
    };
    write_session(&session)?;
    instance.session = Some(session);
    if let Some(window) = window {
        return window(DesktopSettings {
            url: url.parse()?,
            server,
            state,
        });
    }
    println!("{url}");
    serve_settings(&server, &state, None)
}

fn focus_desktop(url: &str) -> Result<()> {
    let mut url = reqwest::Url::parse(url)?;
    let token = url
        .fragment()
        .and_then(|value| value.strip_prefix("token="))
        .context("missing desktop session token")?
        .to_owned();
    url.set_fragment(None);
    url.set_path("/api/focus");
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()?
        .post(url)
        .header("X-SmallTalkAI-Token", token)
        .send()?
        .error_for_status()?;
    Ok(())
}

fn serve_settings(server: &Server, state: &Arc<AppState>, focus: Option<&dyn Fn()>) -> Result<()> {
    let tick = if focus.is_some() { 100 } else { 500 };
    while !state.closing.load(Ordering::Relaxed) {
        if let Some(focus) = focus {
            if state
                .focus_pending
                .as_ref()
                .is_some_and(|pending| pending.swap(false, Ordering::Relaxed))
            {
                focus();
            }
        } else if state.last_activity.lock().expect("activity lock").elapsed() > IDLE_TIMEOUT {
            break;
        }
        if let Some(request) = server.recv_timeout(Duration::from_millis(tick))? {
            *state.last_activity.lock().expect("activity lock") = Instant::now();
            let state = Arc::clone(state);
            std::thread::spawn(move || respond(request, &state));
        }
    }
    Ok(())
}

fn respond(mut request: Request, state: &AppState) {
    let result = handle(&mut request, state);
    let response = match result {
        Ok(response) => response,
        Err(error) => json_response(
            StatusCode(400),
            &json!({"error": format!("{error:#}")}),
            state,
        ),
    };
    let _ = request.respond(response);
}

fn handle(request: &mut Request, state: &AppState) -> Result<Response<std::io::Cursor<Vec<u8>>>> {
    validate_host(request, state)?;
    let path = request.url().split('?').next().unwrap_or(request.url());
    if request.method() == &Method::Get {
        match path {
            "/" | "/index.html" => {
                let theme = AppConfig::load()
                    .map(|config| config.general.ui_theme)
                    .unwrap_or_default();
                let index = INDEX.replacen(
                    "data-theme=\"light\"",
                    &format!("data-theme=\"{}\"", theme.as_str()),
                    1,
                );
                return Ok(asset(&index, "text/html; charset=utf-8", state));
            }
            "/styles.css" => return Ok(asset(CSS, "text/css; charset=utf-8", state)),
            "/settings-sans-license.txt" => {
                return Ok(asset(
                    SETTINGS_FONT_LICENSE,
                    "text/plain; charset=utf-8",
                    state,
                ))
            }
            "/settings-sans.woff2" => {
                return Ok(response_bytes(
                    StatusCode(200),
                    SETTINGS_FONT.to_vec(),
                    "font/woff2",
                    state,
                ))
            }
            "/app.js" => return Ok(asset(JS, "text/javascript; charset=utf-8", state)),
            "/arabic.woff2" => {
                return Ok(response_bytes(
                    StatusCode(200),
                    ARABIC_FONT.to_vec(),
                    "font/woff2",
                    state,
                ))
            }
            "/favicon.ico" => {
                return Ok(response_bytes(
                    StatusCode(200),
                    include_bytes!("../ahk/icons/smalltalkai.ico").to_vec(),
                    "image/x-icon",
                    state,
                ))
            }
            "/smalltalkai.png" | "/smalltalkai-titlebar.png" => {
                let icon: &[u8] = if path == "/smalltalkai-titlebar.png" {
                    include_bytes!("../icons/smalltalkai-titlebar.png")
                } else {
                    include_bytes!("../icons/hicolor/64x64/apps/smalltalkai.png")
                };
                return Ok(response_bytes(
                    StatusCode(200),
                    icon.to_vec(),
                    "image/png",
                    state,
                ));
            }
            _ => {}
        }
    }
    authenticate(request, state)?;
    validate_origin(request, state)?;
    if request.method() == &Method::Post && path == "/api/factory-reset" {
        let _profile = state.profile_lock.write().expect("profile lock");
        let confirmation: Value = serde_json::from_slice(&read_body(request)?)?;
        anyhow::ensure!(
            confirmation.get("confirm") == Some(&Value::Bool(true)),
            "Confirm the factory reset first"
        );
        factory_reset(state)?;
        return Ok(json_response(
            StatusCode(200),
            &json!({"reset":true}),
            state,
        ));
    }
    let _profile = state.profile_lock.read().expect("profile lock");
    match (request.method(), path) {
        (&Method::Get, "/api/state") => state_response(state),
        (&Method::Get, "/api/agent-skills") => {
            let agents = crate::agent_skills::list()?;
            Ok(json_response(
                StatusCode(200),
                &serde_json::to_value(agents)?,
                state,
            ))
        }
        (&Method::Post, "/api/agent-skills") => {
            let agent = serde_json::from_slice(&read_body(request)?)?;
            let installed = crate::agent_skills::install(agent, false)?;
            Ok(json_response(
                StatusCode(200),
                &serde_json::to_value(installed)?,
                state,
            ))
        }
        (method @ (&Method::Get | &Method::Post), "/api/agent-skills/export") => {
            let result = if method == &Method::Get {
                json!(crate::agent_skills::markdown()?)
            } else {
                json!(crate::agent_skills::export()?.to_string_lossy())
            };
            Ok(json_response(StatusCode(200), &result, state))
        }
        (&Method::Get, "/api/modules") => Ok(json_response(
            StatusCode(200),
            &crate::modules::inventory(&AppConfig::load()?),
            state,
        )),
        (&Method::Get, "/api/model-setup") => Ok(json_response(
            StatusCode(200),
            &serde_json::to_value(model_setup()?)?,
            state,
        )),
        (&Method::Post, "/api/model-setup") => {
            let next = serde_json::from_slice(&read_body(request)?)?;
            let next = save_model_setup(next, state)?;
            Ok(json_response(
                StatusCode(200),
                &serde_json::to_value(next)?,
                state,
            ))
        }
        (&Method::Get, "/api/assets") => {
            let catalog = crate::assets::catalog();
            let assets = catalog.iter().map(|asset|json!({"id":asset.id,"label":asset.label,"kind":asset.kind,
                "bytes":asset.bytes + asset.dependencies.iter().filter_map(|id|catalog.iter().find(|a|a.id==*id)).filter(|a|!crate::assets::installed_at(&crate::assets::root(),a)).map(|a|a.bytes).sum::<u64>(),
                "installed":crate::assets::installed_at(&crate::assets::root(),asset) || reader_core::routing::bundled_asset(&asset.id).is_some(),
                "available":crate::assets::available(asset),"directory":crate::assets::directory(&asset.id),"dependencies":asset.dependencies})).collect::<Vec<_>>();
            Ok(json_response(
                StatusCode(200),
                &json!({"assets":assets,"progress":state.assets.status(),"jobs":state.assets.jobs(),"selection_revision":state.assets.selection_revision()}),
                state,
            ))
        }
        (&Method::Post, "/api/asset-action") => {
            let action: Value = serde_json::from_slice(&read_body(request)?)?;
            let id = action["id"].as_str().unwrap_or("");
            match action["action"].as_str().unwrap_or("") {
                "install" => state.assets.start(id)?,
                "cancel" => state.assets.cancel(id),
                "choose" => {
                    let _guard = state.config_lock.lock().expect("config lock");
                    if crate::modules::choose_runtime(id)? {
                        let _ = capture_request(state, ShellCommand::ReloadConfig);
                    }
                    state.assets.selection_changed();
                }
                "remove" => {
                    let _guard = state.config_lock.lock().expect("config lock");
                    if id == crate::assets::NVIDIA_RUNTIME {
                        release_runtime_workers(state)?;
                    }
                    if state.assets.remove(id)? {
                        let _ = capture_request(state, ShellCommand::ReloadConfig);
                    }
                }
                _ => anyhow::bail!("Unknown asset action"),
            }
            Ok(json_response(StatusCode(200), &json!({"ok":true}), state))
        }
        (&Method::Post, "/api/dictation-preview") => {
            let payload: DictationPreview = serde_json::from_slice(&read_body(request)?)?;
            let result = dictation_preview(state, payload)?;
            Ok(json_response(StatusCode(200), &result, state))
        }
        (&Method::Get, "/api/reader") => Ok(json_response(
            StatusCode(200),
            &crate::reader::state()?,
            state,
        )),
        (&Method::Get, "/api/reader-status") => Ok(json_response(
            StatusCode(200),
            &json!({"status":crate::reader::status()}),
            state,
        )),
        (&Method::Get, "/api/reader-output-devices") => Ok(json_response(
            StatusCode(200),
            &json!({"devices":crate::reader::output_devices()?}),
            state,
        )),
        (&Method::Post, "/api/reader-save") => {
            let payload: Value = serde_json::from_slice(&read_body(request)?)?;
            let config: reader_core::AppConfig = serde_json::from_value(payload["config"].clone())?;
            let expected = payload["expected_revision"]
                .as_u64()
                .context("Missing reading revision")?;
            let previous = crate::reader::load()?;
            let online = crate::reader::status().is_some();
            let config = crate::reader::store().save(config, expected)?;
            if online && previous.server != config.server {
                crate::reader::action("server_stop", "")?;
                if config.server.enabled {
                    crate::reader::action("server_start", "")?;
                }
            }
            let _ = capture_request(state, ShellCommand::ReloadConfig);
            Ok(json_response(
                StatusCode(200),
                &json!({"config":config}),
                state,
            ))
        }
        (&Method::Post, "/api/reader-action") => {
            let payload: Value = serde_json::from_slice(&read_body(request)?)?;
            let name = payload["action"]
                .as_str()
                .context("Missing reading action")?;
            let message = crate::reader::action(name, payload["text"].as_str().unwrap_or(""))?;
            Ok(json_response(
                StatusCode(200),
                &json!({"message":message}),
                state,
            ))
        }
        (&Method::Post, "/api/preview-voice") => {
            let payload: Value = serde_json::from_slice(&read_body(request)?)?;
            let binding = serde_json::from_value(payload["binding"].clone())?;
            crate::reader::preview_voice(
                binding,
                payload["language"].as_str().unwrap_or("en"),
                payload["text"].as_str().unwrap_or(""),
                payload["session_id"].as_u64(),
            )?;
            Ok(json_response(StatusCode(200), &json!({"ok":true}), state))
        }
        (&Method::Post, "/api/preview-voice-level") => {
            let payload: Value = serde_json::from_slice(&read_body(request)?)?;
            let session_id = payload["session_id"]
                .as_u64()
                .context("Missing voice preview session")?;
            let rms = crate::reader::preview_voice_rms(session_id);
            let level = crate::capture::audio::visualizer_level_from_rms(rms);
            let waveform = crate::capture::overlay::waveform_for_level(level);
            Ok(json_response(
                StatusCode(200),
                &json!({"waveform":waveform}),
                state,
            ))
        }
        (&Method::Post, "/api/reader-server-key") => {
            let key = reader_core::secrets::server_key()?
                .context("Start the local API before showing its token")?;
            Ok(json_response(StatusCode(200), &json!({"token":key}), state))
        }
        (&Method::Post, "/api/connection-secret") => {
            let payload: Value = serde_json::from_slice(&read_body(request)?)?;
            let slot = payload["slot"]
                .as_str()
                .context("credential slot is required")?;
            reader_core::secrets::validate_slot(slot)?;
            if payload["clear"] == true {
                reader_core::secrets::delete_connection_key(slot)?;
            }
            if let Some(key) = payload["value"]
                .as_str()
                .filter(|key| !key.trim().is_empty())
            {
                reader_core::secrets::set_connection_key(slot, key)?;
            }
            let saved = reader_core::secrets::connection_key(slot)
                .ok()
                .flatten()
                .is_some();
            Ok(json_response(
                StatusCode(200),
                &json!({"saved":saved}),
                state,
            ))
        }
        (&Method::Post, "/api/reader-secrets") => {
            let payload: Value = serde_json::from_slice(&read_body(request)?)?;
            if let Some(key) = payload["stt"].as_str().filter(|k| !k.trim().is_empty()) {
                secrets::set_stt_api_key(key)?;
            }
            if payload["clear_stt"].as_bool() == Some(true) {
                secrets::delete_stt_api_key()?;
            }
            if let Some(key) = payload["speech"].as_str().filter(|k| !k.trim().is_empty()) {
                reader_core::secrets::set_speech_key(key)?;
            }
            if let Some(key) = payload["ocr"].as_str().filter(|k| !k.trim().is_empty()) {
                reader_core::secrets::set_ocr_key(key)?;
            }
            if payload["clear_speech"].as_bool() == Some(true) {
                reader_core::secrets::delete_speech_key()?;
            }
            if payload["clear_ocr"].as_bool() == Some(true) {
                reader_core::secrets::delete_ocr_key()?;
            }
            Ok(json_response(StatusCode(200), &json!({"ok":true}), state))
        }
        (&Method::Post, "/api/provider-discovery") => {
            let discovery: crate::provider_discovery::DiscoveryRequest =
                serde_json::from_slice(&read_body(request)?)?;
            let saved = saved_discovery_key(&discovery)?;
            let result = crate::provider_discovery::discover(&discovery, saved)?;
            Ok(json_response(
                StatusCode(200),
                &serde_json::to_value(result)?,
                state,
            ))
        }
        (&Method::Get, "/api/keyboard-languages") => Ok(json_response(
            StatusCode(200),
            &serde_json::to_value(crate::capture::input_language::keyboard_languages())?,
            state,
        )),
        (&Method::Get, "/api/health") => Ok(json_response(
            StatusCode(200),
            &json!({"ok": !state.closing.load(Ordering::Relaxed), "pid":std::process::id()}),
            state,
        )),
        (&Method::Get, "/api/defaults") => Ok(json_response(
            StatusCode(200),
            &json!({"config": AppConfig::default()}),
            state,
        )),
        (&Method::Get, "/api/events") => events_response(request, state),
        (&Method::Post, "/api/normalize") => {
            let input: Value = serde_json::from_slice(&read_body(request)?)?;
            AppConfig::validate_json_schema(&input)?;
            let config = AppConfig::normalize_json(&input);
            config.validate()?;
            Ok(json_response(
                StatusCode(200),
                &json!({"config": config}),
                state,
            ))
        }
        (&Method::Post, "/api/save") => save_response(request, state),
        (&Method::Post, "/api/action") => action_response(request, state),
        (&Method::Post, "/api/cleanup-action") => cleanup_action_response(request, state),
        (&Method::Post, "/api/platform-action") => platform_action(request, state),
        (&Method::Post, "/api/hotkey-capture") => {
            let options: Value = serde_json::from_slice(&read_body(request)?)?;
            hotkey_capture_response(state, options["allow_caps_lock"].as_bool().unwrap_or(false))
        }
        (&Method::Post, "/api/focus") => {
            let focus = state
                .focus_pending
                .as_ref()
                .context("no desktop Settings window")?;
            focus.store(true, Ordering::Relaxed);
            Ok(json_response(StatusCode(200), &json!({"ok": true}), state))
        }
        (&Method::Post, "/api/close") => {
            state.closing.store(true, Ordering::Relaxed);
            Ok(json_response(StatusCode(200), &json!({"ok": true}), state))
        }
        _ => Ok(json_response(
            StatusCode(405),
            &json!({"error":"method or route not allowed"}),
            state,
        )),
    }
}

fn hotkey_capture_response(
    state: &AppState,
    _allow_caps_lock: bool,
) -> Result<Response<std::io::Cursor<Vec<u8>>>> {
    #[cfg(windows)]
    {
        let result = capture_hotkey_with_ahk(_allow_caps_lock)?;
        Ok(json_response(StatusCode(200), &result, state))
    }
    #[cfg(not(windows))]
    {
        Ok(json_response(
            StatusCode(400),
            &json!({"error":"Shortcuts are assigned by your desktop environment"}),
            state,
        ))
    }
}

#[cfg(windows)]
fn capture_hotkey_with_ahk(allow_caps_lock: bool) -> Result<Value> {
    let (runtime, script) = locate_hotkey_recorder()?;
    let output = std::env::temp_dir().join(format!(
        "smalltalkai-hotkey-{}-{}.txt",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut command = std::process::Command::new(runtime);
    command.arg(script).arg(&output);
    if allow_caps_lock {
        command.arg("--allow-caps-lock");
    }
    let mut child = command
        .spawn()
        .context("starting the AutoHotkey shortcut recorder")?;
    // Release the recorder's global input hook if the Settings process exits.
    let _job = reader_core::windows_job::Job::attach(&child)
        .inspect_err(|_| {
            let _ = child.kill();
            let _ = child.wait();
        })
        .context("binding the shortcut recorder to Settings")?;
    let status = child.wait().context("waiting for the shortcut recorder")?;
    match status.code() {
        Some(1) => return Ok(json!({"cancelled": true})),
        Some(0 | 4) => {}
        _ => anyhow::bail!("shortcut recorder exited with {status}"),
    }
    let key = fs::read_to_string(&output)
        .context("reading the recorded shortcut")?
        .trim_start_matches('\u{feff}')
        .trim()
        .to_owned();
    let _ = fs::remove_file(output);
    anyhow::ensure!(!key.is_empty(), "shortcut recorder did not return a key");
    if status.success() {
        Ok(json!({"hotkey": key}))
    } else {
        Ok(json!({"unmodified_key": key}))
    }
}

#[cfg(windows)]
fn locate_hotkey_recorder() -> Result<(PathBuf, PathBuf)> {
    let exe_dir = std::env::current_exe()?
        .parent()
        .context("settings executable has no parent directory")?
        .to_path_buf();
    let cwd = std::env::current_dir()?;
    let scripts = [
        exe_dir.join("hotkey-recorder.ahk"),
        cwd.join("ahk").join("hotkey-recorder.ahk"),
    ];
    let script = scripts
        .into_iter()
        .find(|path| path.is_file())
        .context("hotkey-recorder.ahk was not found")?;
    let mut runtimes = vec![exe_dir.join("AutoHotkey64.exe")];
    for variable in ["ProgramFiles", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(variable) {
            let base = PathBuf::from(base);
            runtimes.push(base.join("AutoHotkey").join("v2").join("AutoHotkey64.exe"));
            runtimes.push(
                base.join("Programs")
                    .join("AutoHotkey")
                    .join("v2")
                    .join("AutoHotkey64.exe"),
            );
        }
    }
    let runtime = runtimes
        .into_iter()
        .find(|path| path.is_file())
        .context("AutoHotkey v2 was not found")?;
    Ok((runtime, script))
}

fn state_response(state: &AppState) -> Result<Response<std::io::Cursor<Vec<u8>>>> {
    let config_guard = state.config_lock.lock().expect("config lock");
    let path = AppConfig::config_path();
    let mut raw = fs::read(&path).unwrap_or_default();
    let mut config = match AppConfig::load() {
        Ok(config) => config,
        Err(error) => {
            return Ok(json_response(
                StatusCode(200),
                &json!({
                    "config":Value::Null,
                    "config_hash":hash_bytes(&raw),
                    "config_path":path,
                    "config_error":format!("{error:#}"),
                    "ui_localization":crate::localization::state(crate::config::UiLanguage::Auto),
                    "platform":if cfg!(windows){"windows"}else if cfg!(target_os="linux"){"linux"}else{"other"},
                    "service_online":false,
                    "microphones":[],
                    "models":[]
                }),
                state,
            ))
        }
    };
    cleanup::apply_development_env(&mut config.cleanup);
    raw = fs::read(&path)?;
    let models = crate::models::catalog_for_config(&config)
        .into_iter()
        .map(|model| json!({"family":model.family,"quant":model.quant,"file":model.file,"size_mb":model.size_mb,"recommended":model.recommended,"installed":model.installed,"languages":model.languages,"download_url":model.download_url}))
        .collect::<Vec<_>>();
    drop(config_guard);
    let service_online = capture_request(state, ShellCommand::Ping).is_ok();
    let microphones = if service_online {
        capture_request(state, ShellCommand::ListInputs)
            .ok()
            .map(|response| parse_indexed_values(&response, "input"))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let shortcut_state = linux_shortcut_state();
    let cleanup_history = if service_online {
        capture_request(state, ShellCommand::ListCleanupHistory)
            .ok()
            .and_then(|response| response.values.get("history").cloned())
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .unwrap_or_else(|| json!([]))
    } else {
        json!([])
    };
    let compatible_key_saved = secrets::compatible_api_key().ok().flatten().is_some();
    let chatgpt_connected = auth::chatgpt_connected();
    let cleanup_auth_status = state
        .cleanup_auth_status
        .lock()
        .expect("cleanup auth lock")
        .clone();
    Ok(json_response(
        StatusCode(200),
        &json!({
            "config":config,
            "config_hash":hash_bytes(&raw),
            "config_path":AppConfig::config_path(),
            "resolved_runtime_dir":config.parakeet_runtime_dir_path(),
            "runtime_ready":crate::config::parakeet_native_library_candidates(&config.parakeet_runtime_dir_path()).iter().any(|p|p.is_file()),
            "resolved_model_dir":config.model_dir_path(),
            "ui_localization":crate::localization::state(config.general.ui_language),
            "platform":if cfg!(windows){"windows"}else if cfg!(target_os="linux"){"linux"}else{"other"},
            "service_online":service_online,
            "shortcut_state":shortcut_state,
            "linux_hotkeys":linux_hotkey_backend_state(),
            "linux_automation":linux_automation_state(),
            "microphones":microphones,
            "keyboard_languages":crate::capture::input_language::keyboard_languages(),
            "models":models,

            "cleanup": {
                "compatible_key_saved": compatible_key_saved,
                "chatgpt_connected": chatgpt_connected,
                "auth_status": cleanup_auth_status,
                "history": cleanup_history
            }
        }),
        state,
    ))
}

fn cleanup_action_response(
    request: &mut Request,
    state: &AppState,
) -> Result<Response<std::io::Cursor<Vec<u8>>>> {
    let body: CleanupActionRequest = serde_json::from_slice(&read_body(request)?)?;
    let config = if body.config.is_null() {
        AppConfig::load()?
    } else {
        AppConfig::validate_json_schema(&body.config)?;
        AppConfig::normalize_json(&body.config)
    };
    let value = match body.action.as_str() {
        "save_api_key" => {
            secrets::set_compatible_api_key(&body.secret)?;
            json!({"message":"API key saved in the operating-system vault","message_id":"api.keySaved","message_args":{}})
        }
        "delete_api_key" => {
            secrets::delete_compatible_api_key()?;
            json!({"message":"API key removed","message_id":"api.keyRemoved","message_args":{}})
        }
        "list_models" => {
            let models = cleanup::list_models(&config.cleanup)?;
            json!({"message":"Models refreshed","message_id":"api.modelsRefreshed","message_args":{},"models":models})
        }
        "test" => {
            let transcript = if body.transcript.trim().is_empty() {
                "uh hello jason no sorry Jayson comma this is a test".to_owned()
            } else {
                body.transcript
            };
            let result = cleanup::clean_transcript(&config.cleanup, &transcript, None)?;
            json!({"message":"Cleanup test passed","message_id":"api.testPassed","message_args":{},"result":result})
        }
        "chatgpt_login_browser" => {
            let login = auth::begin_browser_login()?;
            let url = login.authorization_url.clone();
            *state.cleanup_auth_status.lock().expect("cleanup auth lock") =
                json!({"state":"waiting_browser"});
            let auth_status = Arc::clone(&state.cleanup_auth_status);
            // The settings process owns this short-lived OAuth wait, while capture remains available.
            std::thread::spawn(move || match login.complete() {
                Ok(_) => {
                    *auth_status.lock().expect("cleanup auth lock") = json!({"state":"connected"})
                }
                Err(error) => {
                    tracing::warn!(%error, "ChatGPT browser login failed");
                    *auth_status.lock().expect("cleanup auth lock") =
                        json!({"state":"error","message":format!("{error:#}")});
                }
            });
            json!({"message":"Finish connecting in your browser","message_id":"api.browserOpened","message_args":{},"url":url})
        }
        "chatgpt_login_code" => {
            let login = auth::begin_device_login()?;
            let url = login.verification_url.clone();
            let code = login.user_code.clone();
            *state.cleanup_auth_status.lock().expect("cleanup auth lock") =
                json!({"state":"waiting_code","code":code});
            let auth_status = Arc::clone(&state.cleanup_auth_status);
            std::thread::spawn(move || match login.complete() {
                Ok(_) => {
                    *auth_status.lock().expect("cleanup auth lock") = json!({"state":"connected"})
                }
                Err(error) => {
                    tracing::warn!(%error, "ChatGPT device login failed");
                    *auth_status.lock().expect("cleanup auth lock") =
                        json!({"state":"error","message":format!("{error:#}")});
                }
            });
            json!({"message":"Enter the code in your browser","message_id":"api.enterCode","message_args":{},"url":url,"code":code})
        }
        "chatgpt_logout" => {
            secrets::delete_chatgpt_tokens()?;
            *state.cleanup_auth_status.lock().expect("cleanup auth lock") = json!({"state":"idle"});
            json!({"message":"ChatGPT disconnected","message_id":"api.disconnected","message_args":{}})
        }
        "clear_history" => {
            let response = capture_request(state, ShellCommand::ClearCleanupHistory)?;
            anyhow::ensure!(response.ok, "{}", response.message);
            json!({"message":"Cleanup history cleared","message_id":"api.historyCleared","message_args":{}})
        }
        _ => anyhow::bail!("unsupported cleanup action"),
    };
    Ok(json_response(StatusCode(200), &value, state))
}

#[cfg(target_os = "linux")]
fn linux_shortcut_state() -> Value {
    fs::read(AppConfig::local_data_dir().join("linux-shortcuts.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_else(|| json!({}))
}

#[cfg(target_os = "linux")]
fn linux_hotkey_backend_state() -> Value {
    fs::read(AppConfig::local_data_dir().join("linux-hotkey-backend.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_else(|| json!({"status":"not_started"}))
}

#[cfg(target_os = "linux")]
fn linux_automation_state() -> Value {
    let exists = |name: &str| {
        std::env::var_os("PATH")
            .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
    };
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let os_release = fs::read_to_string("/etc/os-release").unwrap_or_default();
    let release_value = |key: &str| {
        os_release
            .lines()
            .find_map(|line| {
                let (candidate, value) = line.split_once('=')?;
                (candidate == key).then(|| value.trim_matches('"').to_owned())
            })
            .unwrap_or_default()
    };
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "Unknown desktop".into());
    json!({
        "session": if wayland { "Wayland" } else { "X11" },
        "desktop": desktop,
        "distro": release_value("PRETTY_NAME"),
        "distro_id": release_value("ID"),
        "wl_clipboard": exists("wl-copy") && exists("wl-paste"),
        "recommended": "portal",
        "portal": true,
        "start_command": "systemctl --user start smalltalkai-linux.service",
        "stop_command": "smalltalkai-linux shutdown"
    })
}

#[cfg(not(target_os = "linux"))]
fn linux_automation_state() -> Value {
    json!({})
}

#[cfg(not(target_os = "linux"))]
fn linux_shortcut_state() -> Value {
    json!({})
}

#[cfg(not(target_os = "linux"))]
fn linux_hotkey_backend_state() -> Value {
    json!({})
}

fn release_runtime_workers(state: &AppState) -> Result<()> {
    if state
        .capture
        .as_ref()
        .is_some_and(|capture| capture.state_file.is_file())
    {
        let result = capture_request(state, ShellCommand::PrepareFactoryReset)?;
        anyhow::ensure!(result.ok, "{}", result.message);
    }
    if crate::reader::status().is_some() {
        crate::reader::action("shutdown", "")?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while crate::reader::status().is_some() {
            anyhow::ensure!(
                Instant::now() < deadline,
                "Wait for Read aloud to stop, then try again"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}

fn factory_reset(state: &AppState) -> Result<()> {
    let data = AppConfig::local_data_dir();
    let reader_data = crate::reader::data_dir();
    let mut paths = vec![crate::assets::root(), AppConfig::default().model_dir_path()];
    if reader_data == data.join("reader") {
        paths.push(reader_data);
    }
    let managed = paths
        .iter()
        .filter_map(|path| match fs::symlink_metadata(path) {
            Ok(_) => Some((|| {
                let resolved = fs::canonicalize(path)?;
                let parent = fs::canonicalize(path.parent().context("Data path has no parent")?)?;
                anyhow::ensure!(
                    resolved.parent() == Some(parent.as_path())
                        && resolved.file_name() == path.file_name(),
                    "Data directory redirects elsewhere: {}",
                    path.display()
                );
                Ok(resolved)
            })()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => Some(Err(error.into())),
        })
        .collect::<Result<Vec<_>>>()?;

    state.assets.cancel_and_clear(Duration::from_secs(20))?;
    release_runtime_workers(state)?;
    let _guard = state.config_lock.lock().expect("config lock");
    let shared = crate::config::lock_config()?;
    for path in managed {
        fs::remove_dir_all(&path).with_context(|| format!("deleting {}", path.display()))?;
    }
    for path in [crate::reader::store().path().to_owned(), setup_path()] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("deleting {}", path.display()))
            }
        }
    }
    AppConfig::default().save()?;
    drop(shared);
    if state
        .capture
        .as_ref()
        .is_some_and(|capture| capture.state_file.is_file())
    {
        let result = capture_request(state, ShellCommand::ReloadConfig)?;
        anyhow::ensure!(result.ok, "{}", result.message);
    }
    Ok(())
}

fn save_response(
    request: &mut Request,
    state: &AppState,
) -> Result<Response<std::io::Cursor<Vec<u8>>>> {
    let _config_guard = state.config_lock.lock().expect("config lock");
    let _shared_guard = crate::config::lock_config()?;
    let body: SaveRequest = serde_json::from_slice(&read_body(request)?)?;
    let path = AppConfig::config_path();
    let current = fs::read(&path).unwrap_or_default();
    anyhow::ensure!(
        hash_bytes(&current) == body.expected_hash,
        "config changed outside Settings; reload before saving"
    );
    AppConfig::validate_json_schema(&body.config)?;
    let config = AppConfig::normalize_json(&body.config);
    config.validate()?;
    config.save()?;
    let raw = fs::read(path)?;
    drop(_shared_guard);
    let reloaded = capture_request(state, ShellCommand::ReloadConfig).is_ok();
    Ok(json_response(
        StatusCode(200),
        &json!({"config":config,"config_hash":hash_bytes(&raw),"reloaded":reloaded}),
        state,
    ))
}

fn action_response(
    request: &mut Request,
    state: &AppState,
) -> Result<Response<std::io::Cursor<Vec<u8>>>> {
    let body: ActionRequest = serde_json::from_slice(&read_body(request)?)?;
    let command = match body.action.as_str() {
        "refresh_models" => ShellCommand::RefreshModels,
        "download_model" => ShellCommand::DownloadModel {
            filename: body.filename,
        },
        "cancel_model_download" => ShellCommand::CancelModelDownload {
            filename: body.filename,
        },
        "remove_model" => ShellCommand::RemoveModel {
            filename: body.filename,
        },
        "test_model" => ShellCommand::TestModel {
            filename: if body.filename.is_empty() {
                None
            } else {
                Some(body.filename.clone())
            },
            language: body.language,
        },
        _ => anyhow::bail!("unsupported service action"),
    };
    let response = capture_request(state, command)?;
    if !response.ok {
        return Ok(json_response(
            StatusCode(400),
            &json!({"error":response.message,
                "message_id":response.values.get("message_id"),
                "message_args":response.values.get("message_args").and_then(|args| serde_json::from_str::<Value>(args).ok()).unwrap_or_else(|| json!({}))}),
            state,
        ));
    }
    Ok(json_response(
        StatusCode(200),
        &json!({"message":response.message,
            "message_id":response.values.get("message_id"),
            "message_args":response.values.get("message_args").and_then(|args| serde_json::from_str::<Value>(args).ok()).unwrap_or_else(|| json!({})),
            "values":response.values}),
        state,
    ))
}

fn events_response(
    request: &Request,
    state: &AppState,
) -> Result<Response<std::io::Cursor<Vec<u8>>>> {
    let (_, query) = request.url().split_once('?').unwrap_or_default();
    let parameters =
        url::form_urlencoded::parse(query.as_bytes()).collect::<std::collections::HashMap<_, _>>();
    let after = parameters
        .get("after")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let previous_pid = parameters
        .get("pid")
        .and_then(|value| value.parse::<u32>().ok());
    let capture = state
        .capture
        .as_ref()
        .context("capture service is offline")?;
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let service = ServiceState::load(&capture.state_file)?;
        let restarted = previous_pid.is_some_and(|pid| pid != service.pid);
        let response = capture_command(
            capture,
            &service,
            ShellCommand::PollEvents {
                after_seq: if restarted { 0 } else { after },
            },
        )?;
        if restarted || !response.events.is_empty() || Instant::now() >= deadline {
            let payload =
                serde_json::to_string(&json!({"events":response.events, "pid":service.pid}))?;
            return Ok(response_bytes(
                StatusCode(200),
                format!("event: service-events\ndata: {payload}\n\n").into_bytes(),
                "text/event-stream; charset=utf-8",
                state,
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn platform_action(
    request: &mut Request,
    state: &AppState,
) -> Result<Response<std::io::Cursor<Vec<u8>>>> {
    let body: ActionRequest = serde_json::from_slice(&read_body(request)?)?;
    if body.action == "focused_app" {
        std::thread::sleep(Duration::from_secs(3));
        let identity = focused_app_identity()?;
        return Ok(json_response(
            StatusCode(200),
            &json!({"message":"Focused app detected","message_id":"api.focusedApp","message_args":{},"app_id":identity}),
            state,
        ));
    }
    match body.action.as_str() {
        "open_config" => open_path(&AppConfig::config_path())?,
        "open_config_folder" => open_path(
            AppConfig::config_path()
                .parent()
                .context("config has no parent")?,
        )?,
        "show_shortcuts" => {
            let response = capture_request(state, ShellCommand::ShowShortcuts)?;
            anyhow::ensure!(response.ok, "{}", response.message);
        }
        "configure_shortcuts" => configure_shortcuts()?,
        "sync_shortcuts" => configure_shortcuts()?,
        _ => anyhow::bail!("unsupported platform action"),
    }
    Ok(json_response(
        StatusCode(200),
        &json!({"message":"Request opened","message_id":"api.requestOpened","message_args":{}}),
        state,
    ))
}

#[cfg(target_os = "linux")]
fn focused_app_identity() -> Result<String> {
    let exe = std::env::current_exe()?;
    let helper = [
        exe.parent()
            .unwrap_or(Path::new("."))
            .join("smalltalkai-paste"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/bin/smalltalkai-paste"),
        PathBuf::from("resources/bin/smalltalkai-paste"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .context("focused-app detector is not installed")?;
    let output = Command::new(helper).arg("--active-app").output()?;
    anyhow::ensure!(
        output.status.success(),
        "The focused app does not expose an identity; enter it manually"
    );
    let identity = String::from_utf8(output.stdout)?.trim().to_owned();
    anyhow::ensure!(!identity.is_empty(), "The focused app identity is empty");
    Ok(identity)
}

#[cfg(windows)]
fn focused_app_identity() -> Result<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    unsafe {
        let window = GetForegroundWindow();
        anyhow::ensure!(!window.is_null(), "No foreground application was found");
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        anyhow::ensure!(pid != 0, "The foreground application has no process ID");
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        anyhow::ensure!(
            !process.is_null(),
            "The foreground application cannot be inspected"
        );
        let mut path = vec![0_u16; 32_768];
        let mut len = path.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len);
        CloseHandle(process);
        anyhow::ensure!(ok != 0, "The foreground executable name could not be read");
        let path = String::from_utf16(&path[..len as usize])?;
        let identity = Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .context("The foreground executable has no file name")?
            .to_owned();
        Ok(identity)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn focused_app_identity() -> Result<String> {
    anyhow::bail!("Focused-app detection is unavailable on this platform")
}

#[derive(Deserialize)]
struct DictationPreview {
    action: String,
    session_id: u64,
    language: Option<String>,
}

fn dictation_preview(state: &AppState, request: DictationPreview) -> Result<Value> {
    let session_id = request.session_id;
    let command = match request.action.as_str() {
        "start" => {
            let language = request.language.unwrap_or_else(|| "single".to_owned());
            anyhow::ensure!(
                !language.is_empty()
                    && language.len() <= 16
                    && language
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c == b'-'),
                "Invalid dictation language"
            );
            ShellCommand::StartRecording {
                session_id,
                target_window: None,
                preview_language: Some(language),
            }
        }
        "stop" => ShellCommand::StopRecordingPreview { session_id },
        "waveform" => ShellCommand::RecordingPreviewWaveform { session_id },
        "cancel" => ShellCommand::CancelRecordingPreview { session_id },
        _ => anyhow::bail!("Invalid dictation preview action"),
    };
    let capture = state
        .capture
        .as_ref()
        .context("capture service is offline")?;
    let service = ServiceState::load(&capture.state_file)?;
    let response = capture_command(capture, &service, command)?;
    anyhow::ensure!(response.ok, "{}", response.message);
    if request.action == "waveform" {
        return Ok(
            json!({"waveform": response.values.get("waveform").context("Missing preview waveform")?}),
        );
    }
    if request.action != "stop" {
        return Ok(json!({ "ok": true }));
    }
    let job = session_id.to_string();
    let mut after_seq = 0;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let response = capture_command(capture, &service, ShellCommand::PollEvents { after_seq })?;
        for event in response.events {
            after_seq = event.seq;
            if event.kind != "cli_transcription" || event.values.get("job_id") != Some(&job) {
                continue;
            }
            if let Some(error) = event.values.get("error") {
                anyhow::bail!("{error}");
            }
            let text = event
                .values
                .get("text")
                .context("Missing preview transcript")?;
            anyhow::ensure!(
                !text.trim().is_empty(),
                "{}",
                crate::localization::tr("notice.noSpeech")
            );
            return Ok(json!({ "text": text }));
        }
        anyhow::ensure!(Instant::now() < deadline, "Dictation preview timed out");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn capture_request(state: &AppState, command: ShellCommand) -> Result<ShellResponse> {
    let capture = state
        .capture
        .as_ref()
        .context("capture service is offline")?;
    let service = ServiceState::load(&capture.state_file)?;
    capture_command(capture, &service, command)
}

fn capture_command(
    capture: &CaptureConnection,
    service: &ServiceState,
    command: ShellCommand,
) -> Result<ShellResponse> {
    let token = service
        .authentication_token()
        .map(|token| token.trim().to_owned())
        .or_else(|error| capture.token.clone().context(error))?;
    anyhow::ensure!(
        service.protocol == SHELL_PROTOCOL_VERSION,
        "capture protocol mismatch"
    );
    let mut stream =
        TcpStream::connect_timeout(&service.loopback_address()?, Duration::from_secs(1))?;
    stream.set_read_timeout(Some(Duration::from_secs(6)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    write_json(
        &mut stream,
        &ClientMessage::Hello {
            protocol: SHELL_PROTOCOL_VERSION,
            token,
        },
    )?;
    match read_json::<ServerMessage>(&mut reader)? {
        ServerMessage::HelloAck { .. } => {}
        other => anyhow::bail!("capture handshake failed: {other:?}"),
    }
    let request_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;
    write_json(
        &mut stream,
        &ClientMessage::Command {
            request_id,
            command,
        },
    )?;
    match read_json::<ServerMessage>(&mut reader)? {
        ServerMessage::Response {
            request_id: actual,
            response,
        } if actual == request_id => Ok(response),
        ServerMessage::Error { code, message } => anyhow::bail!("{code}: {message}"),
        other => anyhow::bail!("unexpected capture response: {other:?}"),
    }
}

fn parse_indexed_values(response: &ShellResponse, prefix: &str) -> Vec<Value> {
    response
        .values
        .iter()
        .filter_map(|(key, name)| {
            let index = key
                .strip_prefix(&format!("{prefix}."))?
                .strip_suffix(".label")?;
            let id = response
                .values
                .get(&format!("{prefix}.{index}.id"))
                .cloned()
                .unwrap_or_else(|| name.clone());
            Some(json!({"id":id,"name":name.clone()}))
        })
        .collect()
}

fn saved_discovery_key(
    request: &crate::provider_discovery::DiscoveryRequest,
) -> Result<Option<String>> {
    if let Some(slot) = &request.key_slot {
        reader_core::secrets::validate_slot(slot)?;
        fn matches(value: &Value, slot: &str, url: &str) -> bool {
            match value {
                Value::Object(fields) => {
                    (fields.get("key_slot").and_then(Value::as_str) == Some(slot)
                        && fields
                            .get("base_url")
                            .and_then(Value::as_str)
                            .is_some_and(|saved| {
                                saved.trim().trim_end_matches('/')
                                    == url.trim().trim_end_matches('/')
                            }))
                        || fields.values().any(|value| matches(value, slot, url))
                }
                Value::Array(values) => values.iter().any(|value| matches(value, slot, url)),
                _ => false,
            }
        }
        let config = if request.feature == "stt" {
            serde_json::to_value(AppConfig::load()?.speech.remote_languages)?
        } else if request.feature == "tts" {
            serde_json::to_value(crate::reader::load()?.speech.selection)?
        } else {
            return Ok(None);
        };
        return if matches(&config, slot, &request.base_url) {
            reader_core::secrets::connection_key(slot)
        } else {
            Ok(None)
        };
    }
    let configured = match request.feature.as_str() {
        "stt" => AppConfig::load()?.speech.remote_openai_compatible.base_url,
        "cleanup" => AppConfig::load()?.cleanup.openai_compatible.base_url,
        "tts" => {
            let speech = crate::reader::load()?.speech;
            match speech.provider {
                reader_core::SpeechProvider::OpenAiCompatible { base_url, .. } => base_url,
                _ => match speech.saved_remote {
                    Some(reader_core::SpeechProvider::OpenAiCompatible { base_url, .. }) => {
                        base_url
                    }
                    _ => String::new(),
                },
            }
        }
        "ocr" => {
            let ocr = crate::reader::load()?.ocr;
            match ocr.provider {
                reader_core::OcrProvider::OpenAiCompatible { base_url, .. } => base_url,
                _ => match ocr.saved_remote {
                    Some(reader_core::OcrProvider::OpenAiCompatible { base_url, .. }) => base_url,
                    _ => String::new(),
                },
            }
        }
        _ => String::new(),
    };
    let same = |a: &str, b: &str| a.trim().trim_end_matches('/') == b.trim().trim_end_matches('/');
    if !same(&configured, &request.base_url) {
        return Ok(None);
    }
    Ok(match request.feature.as_str() {
        "stt" => secrets::stt_api_key()?,
        "cleanup" => secrets::compatible_api_key()?,
        "tts" => reader_core::secrets::speech_key()?,
        "ocr" => reader_core::secrets::ocr_key()?,
        _ => None,
    })
}

fn read_body(request: &mut Request) -> Result<Vec<u8>> {
    let length = request.body_length().unwrap_or(0);
    anyhow::ensure!(length <= MAX_BODY, "request body is too large");
    let mut body = Vec::with_capacity(length);
    request
        .as_reader()
        .take((MAX_BODY + 1) as u64)
        .read_to_end(&mut body)?;
    anyhow::ensure!(body.len() <= MAX_BODY, "request body is too large");
    Ok(body)
}

fn authenticate(request: &Request, state: &AppState) -> Result<()> {
    let token = header(request, "Authorization")
        .and_then(|value| value.strip_prefix("Bearer "))
        .or_else(|| header(request, "X-SmallTalkAI-Token"))
        .unwrap_or_default();
    anyhow::ensure!(
        constant_time_eq(token.as_bytes(), state.web_token.as_bytes()),
        "unauthorized"
    );
    Ok(())
}

fn validate_host(request: &Request, state: &AppState) -> Result<()> {
    let hosts = request
        .headers()
        .iter()
        .filter(|header| header.field.equiv("Host"))
        .collect::<Vec<_>>();
    anyhow::ensure!(hosts.len() == 1, "missing or multiple Host headers");
    let expected = state
        .origin
        .strip_prefix("http://")
        .or_else(|| state.origin.strip_prefix("https://"))
        .context("invalid configured origin")?;
    anyhow::ensure!(hosts[0].value.as_str() == expected, "invalid Host header");
    Ok(())
}

fn validate_origin(request: &Request, state: &AppState) -> Result<()> {
    let origins = request
        .headers()
        .iter()
        .filter(|header| header.field.equiv("Origin"))
        .collect::<Vec<_>>();
    anyhow::ensure!(origins.len() <= 1, "multiple Origin headers");
    if let Some(origin) = origins.first() {
        anyhow::ensure!(
            origin.value.as_str() == state.origin,
            "invalid Origin header"
        );
    }
    Ok(())
}

fn header<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str())
}

fn asset(body: &str, content_type: &str, state: &AppState) -> Response<std::io::Cursor<Vec<u8>>> {
    response_bytes(
        StatusCode(200),
        body.as_bytes().to_vec(),
        content_type,
        state,
    )
}

fn json_response(
    status: StatusCode,
    value: &Value,
    state: &AppState,
) -> Response<std::io::Cursor<Vec<u8>>> {
    response_bytes(
        status,
        serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec()),
        "application/json; charset=utf-8",
        state,
    )
}

fn response_bytes(
    status: StatusCode,
    body: Vec<u8>,
    content_type: &str,
    state: &AppState,
) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = Response::from_data(body).with_status_code(status);
    for (name,value) in [
        ("Content-Type",content_type),
        ("Cache-Control","no-store"),
        ("X-Content-Type-Options","nosniff"),
        ("Referrer-Policy","no-referrer"),
        ("Content-Security-Policy","default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; font-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"),
        ("Cross-Origin-Resource-Policy","same-origin"),
    ] { response.add_header(Header::from_bytes(name,value).expect("valid header")); }
    response.add_header(
        Header::from_bytes("Access-Control-Allow-Origin", state.origin.as_str())
            .expect("valid origin"),
    );
    response
}

fn random_token() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn settings_session_path(instance_id: &str) -> PathBuf {
    AppConfig::state_dir().join(format!("settings-session-{instance_id}.json"))
}

fn write_session(session: &SettingsSession) -> Result<()> {
    let path = settings_session_path(&session.instance_id);
    let parent = path.parent().context("settings session has no parent")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut file, session)?;
    file.persist(path)
        .context("saving the private Settings session")?;
    Ok(())
}

fn settings_launch(instance_id: &str) -> Result<SettingsLaunch> {
    let directory = AppConfig::state_dir();
    fs::create_dir_all(&directory)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(format!("settings-instance-{instance_id}.lock")))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match lock.try_lock() {
            Ok(()) => {
                return Ok(SettingsLaunch::Start(SettingsInstance {
                    _lock: lock,
                    session: None,
                }));
            }
            Err(fs::TryLockError::WouldBlock) => {
                if let Some(url) = reusable_session_url(instance_id) {
                    return Ok(SettingsLaunch::Reuse(url));
                }
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "Settings is already running but is not responding; no second server was started"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn reusable_session_url(instance_id: &str) -> Option<String> {
    let session: SettingsSession =
        serde_json::from_slice(&fs::read(settings_session_path(instance_id)).ok()?).ok()?;
    if session.instance_id != instance_id {
        return None;
    }
    let origin = reqwest::Url::parse(&session.origin).ok()?;
    if origin.scheme() != "http"
        || origin.host_str() != Some("127.0.0.1")
        || origin.port().is_none()
        || origin.path() != "/"
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return None;
    }
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?
        .get(format!("{}/api/health", session.origin))
        .header("X-SmallTalkAI-Token", &session.token)
        .send()
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let health: Value = response.json().ok()?;
    if health["ok"] == true && health["pid"].as_u64() == Some(u64::from(session.pid)) {
        Some(format!("{}/#token={}", session.origin, session.token))
    } else {
        None
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right) {
        diff |= a ^ b
    }
    diff == 0
}

fn write_json<T: serde::Serialize>(stream: &mut TcpStream, value: &T) -> Result<()> {
    serde_json::to_writer(&mut *stream, value)?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(())
}
fn read_json<T: serde::de::DeserializeOwned>(reader: &mut impl BufRead) -> Result<T> {
    let mut line = String::new();
    anyhow::ensure!(
        reader.read_line(&mut line)? > 0,
        "capture closed connection"
    );
    Ok(serde_json::from_str(line.trim_end())?)
}

#[cfg(windows)]
fn open_path(path: &Path) -> Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    let target = path
        .to_str()
        .context("path is not Unicode")?
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    anyhow::ensure!(
        result as isize > 32,
        "Windows failed to open the path (ShellExecuteW code {})",
        result as isize
    );
    Ok(())
}
#[cfg(target_os = "linux")]
fn open_path(path: &Path) -> Result<()> {
    let mut child = Command::new("xdg-open").arg(path).spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
#[cfg(not(any(windows, target_os = "linux")))]
fn open_path(_path: &Path) -> Result<()> {
    anyhow::bail!("opening a path is unsupported")
}

#[cfg(target_os = "linux")]
fn configure_shortcuts() -> Result<()> {
    let executable = std::env::current_exe()?
        .parent()
        .context("settings executable has no directory")?
        .join("smalltalkai-linux");
    let output = Command::new(executable)
        .arg("configure-shortcuts")
        .output()
        .context("requesting Linux shortcut configuration")?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn configure_shortcuts() -> Result<()> {
    anyhow::bail!("system shortcut configuration is only available on Linux")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_is_256_bits() {
        assert_eq!(random_token().len(), 64)
    }
    #[test]
    fn constant_time_comparison_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
    }
    #[test]
    fn assets_are_bundled() {
        assert!(INDEX.contains("/app.js"));
        assert!(INDEX.contains("/styles.css"));
        assert!(JS.contains("/api/keyboard-languages"));
        assert!(JS.contains("model_download_progress"));
        assert!(CSS.contains("--accent"));
    }
}
