//! Stable XDG Desktop Portal identity for the resident host application.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const APP_ID: &str = "io.github.yosef0H4.simple_stt";
const DESKTOP_FILE: &str = "io.github.yosef0H4.simple_stt.desktop";

pub async fn register() -> Result<()> {
    ensure_host_desktop_entry()?;
    ashpd::register_host_app(APP_ID.parse()?)
        .await
        .context("registering SmallTalkAI with the XDG portal host registry")
}

pub(super) fn ensure_host_desktop_entry() -> Result<()> {
    let data_home =
        dirs::data_local_dir().context("locating XDG data directory for desktop entry")?;
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    ensure_desktop_entry(&data_home, std::env::split_paths(&data_dirs))
}

fn ensure_desktop_entry(
    data_home: &Path,
    system_data_dirs: impl IntoIterator<Item = PathBuf>,
) -> Result<()> {
    let apps = data_home.join("applications");
    let path = apps.join(DESKTOP_FILE);
    if path.exists()
        || system_data_dirs
            .into_iter()
            .any(|dir| dir.is_absolute() && dir.join("applications").join(DESKTOP_FILE).is_file())
    {
        return Ok(());
    }

    std::fs::create_dir_all(&apps)?;
    let ctl = super::sibling("smalltalkai-readerctl")?;
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=SmallTalkAI\nComment=Local dictation, read aloud, and screen text\nExec=\"{}\" read-selection\nIcon=smalltalkai\nTerminal=false\nCategories=Utility;Accessibility;\n",
        ctl.display()
    );
    std::fs::write(path, desktop)?;
    let _ = Command::new("update-desktop-database")
        .arg(&apps)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_desktop_entry, APP_ID, DESKTOP_FILE};

    #[test]
    fn app_id_matches_packaged_portal_desktop_entry() {
        let desktop = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../packaging/linux")
            .join(DESKTOP_FILE);
        let desktop_id = desktop
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .expect("desktop entry has a UTF-8 file name")
            .to_owned();
        let body = std::fs::read_to_string(desktop).expect("packaged portal desktop entry exists");

        assert_eq!(APP_ID, desktop_id);
        assert!(body.lines().any(|line| line == "Name=SmallTalkAI"));
        assert!(body.lines().any(|line| line == "Icon=smalltalkai"));
    }

    #[test]
    fn reader_does_not_shadow_the_installed_desktop_entry() {
        let temp = tempfile::tempdir().unwrap();
        let data_home = temp.path().join("local");
        let system_data_dir = temp.path().join("system");
        let applications = system_data_dir.join("applications");
        std::fs::create_dir_all(&applications).unwrap();
        std::fs::write(
            applications.join(DESKTOP_FILE),
            "[Desktop Entry]\nType=Application\nName=SmallTalkAI\nExec=smalltalkai launch\n",
        )
        .unwrap();

        ensure_desktop_entry(&data_home, [system_data_dir]).unwrap();

        assert!(!data_home.join("applications").join(DESKTOP_FILE).exists());
    }
}
