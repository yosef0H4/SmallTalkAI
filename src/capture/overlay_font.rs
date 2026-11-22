//! Ships the visualizer font with the app and registers it with fontconfig at
//! runtime, so the overlay's block glyphs (▁▂▃…▇) render correctly even on
//! systems where the font is not installed.
//!
//! A missing font family silently falls back to a proportional font on Linux,
//! which renders the block glyphs with uneven widths/baselines. Bundling our
//! own copy removes that dependency.

use std::ffi::{c_void, CString};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Bundled font: JetBrains Mono (SIL OFL 1.1). See assets/fonts/LICENSE-JetBrainsMono.txt.
const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
const ARABIC_BYTES: &[u8] = include_bytes!("../../assets/fonts/NotoSansArabic-Regular.ttf");
const ICON_BYTES: &[u8] = include_bytes!("../../assets/fonts/SmallTalkAIOverlayIcons-Regular.ttf");
pub(super) const ICON_FAMILY: &str = "SmallTalkAI Overlay Icons";

static REGISTERED: OnceLock<bool> = OnceLock::new();

/// Ensure the bundled font is registered with fontconfig (idempotent). Must be
/// called before the first Pango layout is created so the font map picks it up.
pub fn ensure_registered() -> bool {
    *REGISTERED.get_or_init(|| match register() {
        Ok(()) => {
            tracing::debug!("overlay: registered bundled fonts");
            true
        }
        Err(error) => {
            tracing::warn!(
                "overlay: could not register bundled font ({error}); using system fonts"
            );
            false
        }
    })
}

fn cache_path(filename: &str) -> anyhow::Result<PathBuf> {
    let dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("no cache dir"))?
        .join("smalltalkai")
        .join("fonts");
    Ok(dir.join(filename))
}

fn register() -> anyhow::Result<()> {
    register_font("JetBrainsMono-Regular.ttf", FONT_BYTES)?;
    register_font("NotoSansArabic-Regular.ttf", ARABIC_BYTES)?;
    register_font("SmallTalkAIOverlayIcons-Regular.ttf", ICON_BYTES)
}

fn register_font(filename: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let path = cache_path(filename)?;
    if !std::fs::read(&path).is_ok_and(|cached| cached == bytes) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let staging = crate::config::unique_atomic_temp_path(&path);
        if let Err(error) = std::fs::write(&staging, bytes)
            .map_err(anyhow::Error::from)
            .and_then(|()| crate::config::replace_file_atomic(&staging, &path))
        {
            let _ = std::fs::remove_file(staging);
            return Err(error);
        }
    }

    // FcBool FcConfigAppFontAddFile(FcConfig *config, const FcChar8 *file)
    // config = NULL means the current configuration. fontconfig is already
    // loaded into the process by pango, so we dlopen it rather than link it.
    unsafe {
        let lib = libloading::Library::new("libfontconfig.so.1")
            .or_else(|_| libloading::Library::new("libfontconfig.so"))?;
        let add: libloading::Symbol<unsafe extern "C" fn(*const c_void, *const u8) -> i32> =
            lib.get(b"FcConfigAppFontAddFile\0")?;
        let cpath = CString::new(path.to_string_lossy().as_bytes())?;
        let ok = add(std::ptr::null(), cpath.as_ptr() as *const u8);
        // Keep the library mapped for the process lifetime.
        std::mem::forget(lib);
        if ok == 0 {
            anyhow::bail!(
                "FcConfigAppFontAddFile returned false for {}",
                path.display()
            );
        }
    }
    Ok(())
}
