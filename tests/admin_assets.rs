use smalltalkai::config::AppConfig;
use std::process::Command;

#[test]
fn headless_asset_install_accepts_an_existing_stt_model_without_capture() {
    let temporary = tempfile::tempdir().unwrap();
    let model = "tdt_ctc-110m-q8_0.gguf";
    let models = temporary.path().join("models");
    std::fs::create_dir(&models).unwrap();
    // No inference occurs: installation must preserve an already present model.
    std::fs::write(models.join(model), b"existing model").unwrap();
    let mut config = AppConfig::default();
    config.speech.model_dir = models.to_string_lossy().into_owned();
    let path = temporary.path().join("config.json");
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_smalltalkai-ctl"))
        .args(["assets", "install", model])
        .env("SIMPLE_STT_CONFIG", path)
        .env("XDG_DATA_HOME", temporary.path().join("data"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["installed"], model);
    assert_eq!(
        std::fs::read(models.join(model)).unwrap(),
        b"existing model"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn headless_removal_clears_inactive_bindings_and_shared_runtime_assignments() {
    use reader_core::routing::Binding;
    use std::fs;

    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let reader = reader_core::ConfigStore::new(root.join("reader/config.json"));
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_smalltalkai-ctl"))
            .args(args)
            .env("SIMPLE_STT_CONFIG", root.join("config.json"))
            .env("SIMPLE_STT_READER_DATA", root.join("reader"))
            .env("XDG_DATA_HOME", root.join("data"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let inventory = run(&["assets", "list"]);
    let install_fixture = |id: &str| {
        let asset = inventory["assets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|asset| asset["id"] == id)
            .unwrap();
        let path = std::path::PathBuf::from(asset["directory"].as_str().unwrap());
        assert!(path.starts_with(root));
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join(".verified"),
            asset["asset"]["sha256"].as_str().unwrap(),
        )
        .unwrap();
        path
    };
    let model = install_fixture("supertonic3");
    let runtime = install_fixture("sherpa-cpu");
    let mut config = reader.load().unwrap();
    config.speech.selection.languages.insert(
        "ar".into(),
        Binding {
            model: Some("supertonic3".into()),
            runtime: Some("sherpa-cpu".into()),
            voice: Some("sid:2".into()),
            steps: Some(6),
            ..Default::default()
        },
    );
    config.ocr.selection.single.runtime = Some("paddle-cpu".into());
    config.ocr.selection.single.model = Some("paddle-ar-en".into());
    reader.save(config, 1).unwrap();

    run(&["assets", "remove", "supertonic3"]);
    assert!(!model.exists());
    let config = reader.load().unwrap();
    let arabic = &config.speech.selection.languages["ar"];
    assert_eq!(arabic.model, None);
    assert_eq!(arabic.voice, None);
    assert_eq!(arabic.steps, None);
    assert_eq!(arabic.runtime.as_deref(), Some("sherpa-cpu"));
    assert_eq!(config.speech.selection.single, Binding::default());
    assert_eq!(
        config.ocr.selection.single.model.as_deref(),
        Some("paddle-ar-en")
    );
    assert!(runtime.is_dir());

    run(&["assets", "remove", "sherpa-cpu"]);
    assert!(!runtime.exists());
    let config = reader.load().unwrap();
    assert_eq!(config.speech.selection.languages["ar"].runtime, None);
    assert_eq!(config.ocr.selection.single.runtime, None);
    assert_eq!(config.ocr.selection.single.model, None);
}
