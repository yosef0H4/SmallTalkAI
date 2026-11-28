#![cfg_attr(windows, windows_subsystem = "windows")]

use anyhow::Result;
use clap::Parser;
use smalltalkai::{
    config::{AppConfig, UiTheme},
    settings_server::{self, Args, DesktopSettings},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{
    webview::NewWindowResponse, window::Color, Manager, Theme, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

// Match the Settings --paper colors while the WebView composites its first frame.
const LIGHT_BACKGROUND: Color = Color(216, 216, 216, 255);
const DARK_BACKGROUND: Color = Color(48, 48, 48, 255);

fn main() {
    if let Err(error) = settings_server::run(Args::parse(), Some(open_window)) {
        eprintln!("smalltalkai-settings-window: {error:#}");
        std::process::exit(1);
    }
}

fn open_window(settings: DesktopSettings) -> Result<()> {
    #[cfg(target_os = "linux")]
    // GTK uses the program name as the Wayland app ID. Match the installed
    // desktop entry without making GTK register a single-instance bus name.
    glib::set_prgname(Some(smalltalkai::config::LINUX_DESKTOP_ID));
    let data_dir = AppConfig::local_data_dir().join("settings-webview");
    let theme = AppConfig::load()
        .map(|config| config.general.ui_theme)
        .unwrap_or_default();
    let background = if theme == UiTheme::Light {
        LIGHT_BACKGROUND
    } else {
        DARK_BACKGROUND
    };
    let url = settings.url.clone();
    #[cfg(debug_assertions)]
    let url = settings_dev_url(url)?;
    let origin = url.origin();
    tauri::Builder::default()
        .setup(move |app| {
            let loaded = Arc::new(AtomicBool::new(false));
            let page_loaded = loaded.clone();
            let shown = Arc::new(AtomicBool::new(false));
            let page_shown = shown.clone();
            let ready_shown = shown.clone();
            let page_origin = origin.clone();
            let frame_origin = origin.clone();
            let close = app.handle().clone();
            let frame_window = close.clone();
            let window = WebviewWindowBuilder::new(app, "settings", WebviewUrl::External(url))
                .title("SmallTalk")
                .inner_size(1024.0, 760.0)
                .min_inner_size(320.0, 360.0)
                .decorations(false)
                .visible(false)
                .background_color(background)
                .data_directory(data_dir)
                .initialization_script(
                    "window.__SMALLTALKAI_DESKTOP__ = true;
                    window.addEventListener('settings-close-approved', (event) => {
                        event.preventDefault();
                        window.location.href = '/__settings_close';
                    });",
                )
                .on_navigation(move |url| {
                    if url.origin() != origin {
                        return false;
                    }
                    if url.path() == "/__settings_close" {
                        // Leave the webview callback before destroying its window.
                        let handle = close.clone();
                        std::thread::spawn(move || {
                            if let Some(window) = handle.get_webview_window("settings") {
                                let _ = window.destroy();
                            }
                        });
                        return false;
                    }
                    if url.path() == "/__settings_frame/ready" {
                        let handle = close.clone();
                        if !ready_shown.swap(true, Ordering::Relaxed) {
                            let _ = close.run_on_main_thread(move || {
                                if let Some(window) = handle.get_webview_window("settings") {
                                    if theme == UiTheme::Auto
                                        && window.theme().ok() == Some(Theme::Light)
                                    {
                                        let _ = window.set_background_color(Some(LIGHT_BACKGROUND));
                                    }
                                    if let Err(error) = window.show() {
                                        eprintln!("smalltalkai-settings-window: show: {error}");
                                    }
                                }
                            });
                        }
                        return false;
                    }
                    true
                })
                .on_new_window(move |url, _| {
                    if url.origin() == frame_origin {
                        let action = match url.path() {
                            "/__settings_frame/drag" => "drag",
                            "/__settings_frame/minimize" => "minimize",
                            "/__settings_frame/maximize" => "maximize",
                            _ => return NewWindowResponse::Deny,
                        };
                        let handle = frame_window.clone();
                        let _ = frame_window.run_on_main_thread(move || {
                            if let Some(window) = handle.get_webview_window("settings") {
                                if let Err(error) = frame_action(&window, action) {
                                    eprintln!("smalltalkai-settings-window: {action}: {error}");
                                }
                            }
                        });
                    } else if url.scheme() == "https" {
                        // The cleanup OAuth flow uses the user's normal browser session.
                        let _ = open::that_detached(url.as_str());
                    }
                    NewWindowResponse::Deny
                })
                .on_page_load(move |window, payload| {
                    use tauri::webview::PageLoadEvent;
                    if payload.url().origin() != page_origin {
                        return;
                    }
                    let finished = payload.event() == PageLoadEvent::Finished;
                    page_loaded.store(finished, Ordering::Relaxed);
                    if finished {
                        publish_maximized(&window);
                        if !page_shown.load(Ordering::Relaxed) {
                            // A hidden WebView can defer animation frames.
                            let _ = window.eval(
                                "(() => {
                                    const ready = () => window.location.replace('/__settings_frame/ready');
                                    const fallback = setTimeout(ready, 250);
                                    requestAnimationFrame(() => requestAnimationFrame(() => {
                                        clearTimeout(fallback);
                                        ready();
                                    }));
                                })()",
                            );
                        }
                    }
                })
                .build()?;
            let target = window.clone();
            window.on_window_event(move |event| {
                if matches!(event, WindowEvent::Resized(_) | WindowEvent::Focused(true))
                    && loaded.load(Ordering::Relaxed)
                {
                    publish_maximized(&target);
                }
                if let WindowEvent::CloseRequested { api, .. } = event {
                    if loaded.load(Ordering::Relaxed)
                        && target
                            .eval("window.dispatchEvent(new Event('settings-close-request'))")
                            .is_ok()
                    {
                        api.prevent_close();
                    }
                }
            });
            let focus_window = window.clone();
            settings.serve(
                move || {
                    if !shown.load(Ordering::Relaxed) {
                        return;
                    }
                    let target = focus_window.clone();
                    let _ = focus_window.run_on_main_thread(move || {
                        let _ = target.unminimize();
                        let _ = target.set_focus();
                    });
                },
                move || {
                    let _ = window.destroy();
                },
            );
            Ok(())
        })
        .build(tauri::generate_context!())?
        .run_return(|_, _| {});
    Ok(())
}

fn publish_maximized(window: &tauri::WebviewWindow) {
    if let Ok(maximized) = window.is_maximized() {
        publish_frame_state(window, maximized);
    }
}

fn publish_frame_state(window: &tauri::WebviewWindow, maximized: bool) {
    let _ = window.eval(format!(
        "document.documentElement.dataset.maximized = '{maximized}'; \
        window.dispatchEvent(new Event('settings-window-state'));"
    ));
}

fn frame_action(window: &tauri::WebviewWindow, action: &str) -> tauri::Result<()> {
    match action {
        "drag" => window.start_dragging(),
        "minimize" => window.minimize(),
        "maximize" => {
            let maximize = !window.is_maximized()?;
            if maximize {
                window.maximize()?;
            } else {
                window.unmaximize()?;
            }
            publish_frame_state(window, maximize);
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(debug_assertions)]
fn settings_dev_url(url: tauri::Url) -> Result<tauri::Url> {
    let Ok(origin) = std::env::var("SMALLTALKAI_SETTINGS_DEV_ORIGIN") else {
        return Ok(url);
    };
    let mut dev = tauri::Url::parse(&origin)?;
    anyhow::ensure!(
        dev.scheme() == "http"
            && dev.host_str() == Some("127.0.0.1")
            && dev.port().is_some()
            && dev.username().is_empty()
            && dev.password().is_none()
            && dev.path() == "/"
            && dev.query().is_none()
            && dev.fragment().is_none(),
        "SMALLTALKAI_SETTINGS_DEV_ORIGIN must be an HTTP loopback origin with a port"
    );
    dev.set_fragment(url.fragment());
    Ok(dev)
}
