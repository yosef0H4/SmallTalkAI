//! Reading uses the same native overlay and fonts as dictation.
use anyhow::Result;
use smalltalkai::capture::overlay::OverlayHandle;
use std::time::{Duration, Instant};
fn main() -> Result<()> {
    #[cfg(windows)]
    let overlay = OverlayHandle::spawn_reader()?;
    #[cfg(not(windows))]
    let overlay = OverlayHandle::spawn()?;
    let config = smalltalkai::config::AppConfig::load()?;
    #[cfg(target_os = "linux")]
    let (config_path, mut config_modified) = {
        overlay.set_theme(config.general.ui_theme);
        let path = smalltalkai::config::AppConfig::config_path();
        let modified = path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok();
        (path, modified)
    };
    overlay.set_locale(smalltalkai::localization::resolve(
        config.general.ui_language,
    ));
    overlay.set_shortcuts(
        smalltalkai::shortcuts::summary(
            &smalltalkai::shortcuts::assigned(&config),
            smalltalkai::localization::resolve(config.general.ui_language),
        ),
        config.general.shortcut_reminders,
    );
    let locale = smalltalkai::localization::resolve(config.general.ui_language);
    // This local progress HUD shows the current passage, as requested. The
    // separate system-notification privacy policy still hides source text.
    let started = Instant::now();
    let mut previous = String::new();
    let mut idle = Instant::now();
    let mut hints_enabled = config.general.shortcut_reminders;
    while let Some(status) = smalltalkai::reader::status() {
        #[cfg(target_os = "linux")]
        if let Some(modified) = config_path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .filter(|modified| Some(*modified) != config_modified)
        {
            if let Ok(updated) = smalltalkai::config::AppConfig::load_from(&config_path) {
                overlay.set_theme(updated.general.ui_theme);
                config_modified = Some(modified);
            }
        }
        let next_hints_enabled =
            smalltalkai::reader::shortcut_hints_enabled(&status, config.general.shortcut_reminders);
        if hints_enabled != next_hints_enabled {
            overlay.set_shortcuts(
                smalltalkai::shortcuts::summary(&smalltalkai::shortcuts::assigned(&config), locale),
                next_hints_enabled,
            );
            hints_enabled = next_hints_enabled;
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let (text, active) = smalltalkai::reader::hud_text(&status, locale, now_ms);
        if active {
            idle = Instant::now();
        }
        if text != previous {
            if text.is_empty() {
                overlay.hide();
            } else {
                overlay.notify_info(text.clone(), None);
            }
            previous = text;
        }
        if started.elapsed() > Duration::from_secs(3) && idle.elapsed() > Duration::from_secs(3) {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    overlay.hide();
    Ok(())
}
