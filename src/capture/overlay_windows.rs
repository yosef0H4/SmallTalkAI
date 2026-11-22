use crate::capture::overlay::overlay_model::{
    empty_visualizer_levels, set_visualizer_level, windows_tooltip_text, OverlayCommand,
    OverlayContent, OverlayPrimary, RecordingIndicators, VisualizerLevels,
};
use crate::localization::Locale;
use anyhow::{anyhow, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use reader_core::local_ipc::{WINDOWS_PICKER_CLASS, WINDOWS_TOOLTIP_PROPERTY};
use std::mem::zeroed;
use std::ptr::null_mut;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Once,
};
use std::thread;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    AddFontMemResourceEx, CreateFontIndirectW, DeleteObject, GetMonitorInfoW, GetObjectW,
    GetStockObject, MonitorFromPoint, DEFAULT_GUI_FONT, HFONT, LOGFONTW, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, SetEvent, WaitForSingleObject, INFINITE,
};
use windows_sys::Win32::UI::Controls::{
    InitCommonControls, TOOLTIPS_CLASSW, TTF_ABSOLUTE, TTF_RTLREADING, TTF_TRACK, TTM_ADDTOOLW,
    TTM_ADJUSTRECT, TTM_SETMAXTIPWIDTH, TTM_TRACKACTIVATE, TTM_TRACKPOSITION, TTM_UPDATETIPTEXTW,
    TTS_ALWAYSTIP, TTS_NOANIMATE, TTS_NOFADE, TTS_NOPREFIX, TTTOOLINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, EnumWindows, FindWindowW, GetCursorPos,
    GetPropW, GetWindowRect, IsWindow, IsWindowVisible, PeekMessageW, SendMessageW, SetPropW,
    TranslateMessage, MSG, PM_REMOVE, WM_GETFONT, WM_SETFONT, WS_EX_TOPMOST, WS_POPUP,
};

const CURSOR_OFFSET: i32 = 16;
const NOTICE_POLL_INTERVAL: Duration = Duration::from_millis(25);
const RECORDING_POLL_INTERVAL: Duration = Duration::from_millis(33);
const MAX_TOOLTIP_WIDTH: i32 = 420;

#[derive(Debug, Clone)]
pub struct OverlayHandle {
    sender: Arc<OverlaySender>,
    latest_level: Arc<AtomicU32>,
}

#[derive(Debug)]
struct OverlaySender {
    tx: Sender<WindowsCommand>,
    wake: Arc<WakeEvent>,
}

impl OverlaySender {
    fn send(&self, command: WindowsCommand) -> Result<()> {
        self.tx.send(command)?;
        unsafe { SetEvent(self.wake.handle()) };
        Ok(())
    }
}

impl Drop for OverlaySender {
    fn drop(&mut self) {
        let _ = self.send(WindowsCommand::Stop);
    }
}

#[derive(Debug)]
struct WakeEvent(isize);

impl WakeEvent {
    fn handle(&self) -> HANDLE {
        self.0 as HANDLE
    }
}

impl Drop for WakeEvent {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle()) };
    }
}

fn profile_name(prefix: &str) -> Vec<u16> {
    format!("{prefix}{}", crate::config::app_instance_id())
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

#[derive(Debug, Clone)]
enum WindowsCommand {
    Stop,
    Update(OverlayCommand),
    Probe(Instant, Sender<Duration>),
}

impl OverlayHandle {
    pub fn spawn() -> Result<Self> {
        Self::spawn_for(false)
    }

    pub fn spawn_reader() -> Result<Self> {
        Self::spawn_for(true)
    }

    fn spawn_for(reader: bool) -> Result<Self> {
        let (tx, rx) = unbounded::<WindowsCommand>();
        let name = profile_name("Local\\SimpleSttShortcutWake-");
        let event = unsafe {
            CreateEventW(
                std::ptr::null(),
                0,
                0,
                if reader {
                    std::ptr::null()
                } else {
                    name.as_ptr()
                },
            )
        };
        if event.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let wake = Arc::new(WakeEvent(event as isize));
        let latest_level = Arc::new(AtomicU32::new(0.0_f32.to_bits()));
        thread::spawn({
            let latest_level = Arc::clone(&latest_level);
            let wake = Arc::clone(&wake);
            move || overlay_thread(rx, latest_level, wake, reader)
        });
        Ok(Self {
            sender: Arc::new(OverlaySender { tx, wake }),
            latest_level,
        })
    }

    pub(super) fn send(&self, command: OverlayCommand) {
        let _ = self.sender.send(WindowsCommand::Update(command));
    }

    /// Backwards-compatible alias used by the screenshot and latency helpers.
    pub fn show(&self, target_window: isize) {
        self.start_recording(target_window, RecordingIndicators::default());
    }

    pub fn set_level(&self, level: f32) {
        self.latest_level
            .store(level.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn level_cell(&self) -> Arc<AtomicU32> {
        Arc::clone(&self.latest_level)
    }

    pub fn benchmark_latency(&self, iterations: usize) -> Vec<Duration> {
        let mut values = Vec::with_capacity(iterations);
        for index in 0..iterations {
            self.set_level((index % 2) as f32);
            let (tx, rx) = crossbeam_channel::bounded(1);
            if self
                .sender
                .send(WindowsCommand::Probe(Instant::now(), tx))
                .is_err()
            {
                break;
            }
            if let Ok(value) = rx.recv_timeout(Duration::from_secs(1)) {
                values.push(value);
            }
        }
        values
    }
}

fn overlay_thread(
    rx: Receiver<WindowsCommand>,
    latest_level: Arc<AtomicU32>,
    wake: Arc<WakeEvent>,
    reader: bool,
) {
    let mut state = TooltipState::new();
    state.reader = reader;
    let hint_window = profile_name("SimpleSttShortcutHold-");
    loop {
        while let Ok(command) = rx.try_recv() {
            if matches!(command, WindowsCommand::Stop) {
                return;
            }
            state.handle_command(command);
        }
        pump_messages();
        let publisher = unsafe { FindWindowW(std::ptr::null(), hint_window.as_ptr()) };
        state.shortcuts_held = !publisher.is_null()
            && !unsafe { GetPropW(publisher, windows_sys::w!("SimpleSttShortcutHold")) }.is_null();
        state.set_level(f32::from_bits(latest_level.load(Ordering::Relaxed)));
        state.tick();
        let timeout = if state.should_render() || (!reader && state.shortcuts_held) {
            state.poll_interval().as_millis() as u32
        } else {
            INFINITE
        };
        unsafe { WaitForSingleObject(wake.handle(), timeout) };
    }
}

fn pump_messages() {
    unsafe {
        let mut msg: MSG = zeroed();
        while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

struct TooltipState {
    hwnd: HWND,
    arabic_font: HFONT,
    owner_hwnd: HWND,
    tool: TTTOOLINFOW,
    content: OverlayContent,
    shortcuts_held: bool,
    reader: bool,
    reader_marker: Vec<u16>,
    target_level: f32,
    display_level: f32,
    visualizer_levels: VisualizerLevels,
    last_text: String,
    text_buf: Vec<u16>,
    active: bool,
    last_position: Option<(i32, i32)>,
    max_width: i32,
}

impl TooltipState {
    fn new() -> Self {
        unsafe {
            InitCommonControls();
        }
        Self {
            hwnd: null_mut(),
            arabic_font: null_mut(),
            owner_hwnd: null_mut(),
            tool: unsafe { zeroed() },
            content: OverlayContent::default(),
            shortcuts_held: false,
            reader: false,
            reader_marker: profile_name("SimpleSttReaderOverlay-"),
            target_level: 0.0,
            display_level: 0.0,
            visualizer_levels: empty_visualizer_levels(),
            last_text: String::new(),
            text_buf: Vec::new(),
            active: false,
            last_position: None,
            max_width: 0,
        }
    }

    fn handle_command(&mut self, command: WindowsCommand) {
        match command {
            WindowsCommand::Update(command) => {
                let reset_window = matches!(command, OverlayCommand::Hide)
                    || matches!(command, OverlayCommand::SetLocale(locale) if self.content.locale != locale);
                if command.resets_waveform() {
                    self.target_level = 0.0;
                    self.display_level = 0.0;
                    self.visualizer_levels = empty_visualizer_levels();
                }
                self.content.apply(command, Instant::now());
                if reset_window {
                    self.destroy_tooltip();
                }
            }
            WindowsCommand::Stop => self.destroy_tooltip(),
            WindowsCommand::Probe(sent_at, tx) => {
                let _ = tx.try_send(sent_at.elapsed());
            }
        }
    }

    fn set_level(&mut self, level: f32) {
        if self.content.primary == OverlayPrimary::Recording {
            self.target_level = level.clamp(0.0, 1.0);
        }
    }

    fn should_render(&self) -> bool {
        self.content.has_status()
            || (!self.reader
                && self.shortcuts_held
                && self.content.has_shortcuts()
                && !self.reader_visible())
    }

    fn reader_visible(&self) -> bool {
        unsafe extern "system" fn check(hwnd: HWND, marker: LPARAM) -> i32 {
            unsafe {
                (IsWindowVisible(hwnd) == 0 || GetPropW(hwnd, marker as *const u16).is_null())
                    as i32
            }
        }
        unsafe { EnumWindows(Some(check), self.reader_marker.as_ptr() as LPARAM) == 0 }
    }

    fn poll_interval(&self) -> Duration {
        if self.content.primary == OverlayPrimary::Recording {
            RECORDING_POLL_INTERVAL
        } else {
            NOTICE_POLL_INTERVAL
        }
    }

    fn tick(&mut self) {
        self.content.expire_notice(Instant::now());
        if !self.should_render()
            || !unsafe { FindWindowW(WINDOWS_PICKER_CLASS, std::ptr::null()) }.is_null()
        {
            self.destroy_tooltip();
            return;
        }
        if let Err(error) = self.ensure_tooltip() {
            tracing::warn!(%error, "native tooltip overlay failed to open");
            self.content.apply(OverlayCommand::Hide, Instant::now());
            return;
        }

        if self.content.primary == OverlayPrimary::Recording {
            self.display_level = self.display_level * 0.10 + self.target_level * 0.90;
            set_visualizer_level(&mut self.visualizer_levels, self.display_level);
        }
        let text = windows_tooltip_text(&self.render_text(), self.content.locale);
        if text != self.last_text {
            self.set_text(&text);
            self.last_text = text;
        }

        let mut cursor = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut cursor);
        }
        let mut point = POINT {
            x: cursor.x + CURSOR_OFFSET,
            y: cursor.y + CURSOR_OFFSET,
        };
        let work_area = monitor_work_area(point);

        self.update_max_width(work_area);
        let size = self.tooltip_size();
        if point.x + size.0 >= work_area.right {
            point.x = work_area.right - size.0 - 1;
        }
        if point.y + size.1 >= work_area.bottom {
            point.y = work_area.bottom - size.1 - 1;
        }
        if cursor_inside(point, size, cursor) {
            point.x = cursor.x - size.0 - 3;
            point.y = cursor.y - size.1 - 3;
        }

        if self.last_position != Some((point.x, point.y)) {
            unsafe {
                SendMessageW(
                    self.hwnd,
                    TTM_TRACKPOSITION,
                    0,
                    make_lparam(point.x, point.y),
                );
            }
            self.last_position = Some((point.x, point.y));
        }
        if !self.active {
            unsafe {
                SendMessageW(
                    self.hwnd,
                    TTM_TRACKACTIVATE,
                    1,
                    &self.tool as *const TTTOOLINFOW as LPARAM,
                );
            }
            self.active = true;
        }
    }

    fn render_text(&self) -> String {
        self.content.text(
            &self.visualizer_levels,
            self.shortcuts_held && (!self.reader || self.content.has_status()),
        )
    }

    fn ensure_tooltip(&mut self) -> Result<()> {
        if !self.hwnd.is_null() && unsafe { IsWindow(self.hwnd) != 0 } {
            return Ok(());
        }
        self.destroy_tooltip();
        self.text_buf = "\0".encode_utf16().collect();
        self.owner_hwnd = unsafe {
            CreateWindowExW(
                0,
                windows_sys::w!("STATIC"),
                windows_sys::w!("SimpleSttTooltipOwner"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if self.owner_hwnd.is_null() {
            self.destroy_tooltip();
            return Err(anyhow!("CreateWindowExW tooltip owner failed"));
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST,
                TOOLTIPS_CLASSW,
                null_mut(),
                WS_POPUP | TTS_NOPREFIX | TTS_ALWAYSTIP | TTS_NOANIMATE | TTS_NOFADE,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if hwnd.is_null() {
            self.destroy_tooltip();
            return Err(anyhow!("CreateWindowExW TOOLTIPS_CLASSW failed"));
        }
        self.hwnd = hwnd;
        if self.reader {
            unsafe { SetPropW(hwnd, self.reader_marker.as_ptr(), 1_usize as _) };
        }
        if unsafe { SetPropW(hwnd, WINDOWS_TOOLTIP_PROPERTY, 1_usize as _) } == 0 {
            self.destroy_tooltip();
            return Err(anyhow!("Could not mark the SmallTalkAI tooltip"));
        }
        if self.content.locale == Locale::Ar {
            self.arabic_font = native_arabic_font(hwnd);
            if !self.arabic_font.is_null() {
                unsafe { SendMessageW(hwnd, WM_SETFONT, self.arabic_font as usize, 0) };
            }
        }
        self.tool = TTTOOLINFOW {
            cbSize: tooltip_info_size(),
            uFlags: TTF_TRACK
                | TTF_ABSOLUTE
                | if self.content.locale == Locale::Ar {
                    TTF_RTLREADING
                } else {
                    0
                },
            hwnd: self.owner_hwnd,
            uId: 1,
            rect: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            hinst: null_mut(),
            lpszText: self.text_buf.as_mut_ptr(),
            lParam: 0,
            lpReserved: null_mut(),
        };
        unsafe {
            let added = SendMessageW(
                self.hwnd,
                TTM_ADDTOOLW,
                0,
                &self.tool as *const TTTOOLINFOW as LPARAM,
            );
            if added == 0 {
                self.destroy_tooltip();
                return Err(anyhow!("TTM_ADDTOOLW failed"));
            }
        }
        tracing::debug!("native tooltip overlay opened");
        Ok(())
    }

    fn set_text(&mut self, text: &str) {
        self.text_buf = text.encode_utf16().chain(Some(0)).collect();
        self.tool.lpszText = self.text_buf.as_mut_ptr();
        if !self.hwnd.is_null() {
            unsafe {
                SendMessageW(
                    self.hwnd,
                    TTM_UPDATETIPTEXTW,
                    0,
                    &self.tool as *const TTTOOLINFOW as LPARAM,
                );
            }
        }
    }

    fn update_max_width(&mut self, work_area: RECT) {
        let mut text_rect = work_area;
        unsafe {
            SendMessageW(
                self.hwnd,
                TTM_ADJUSTRECT,
                0,
                &mut text_rect as *mut RECT as LPARAM,
            );
            let width = (text_rect.right - text_rect.left).clamp(160, MAX_TOOLTIP_WIDTH);
            if width != self.max_width {
                SendMessageW(self.hwnd, TTM_SETMAXTIPWIDTH, 0, width as LPARAM);
                self.max_width = width;
            }
        }
    }

    fn tooltip_size(&self) -> (i32, i32) {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 160,
            bottom: 32,
        };
        unsafe {
            GetWindowRect(self.hwnd, &mut rect);
        }
        (
            (rect.right - rect.left).max(32),
            (rect.bottom - rect.top).max(20),
        )
    }

    fn destroy_tooltip(&mut self) {
        self.active = false;
        self.last_position = None;
        self.max_width = 0;
        if !self.hwnd.is_null() {
            unsafe {
                DestroyWindow(self.hwnd);
            }
            self.hwnd = null_mut();
            self.last_text.clear();
        }
        if !self.arabic_font.is_null() {
            unsafe { DeleteObject(self.arabic_font) };
            self.arabic_font = null_mut();
        }
        if !self.owner_hwnd.is_null() {
            unsafe {
                DestroyWindow(self.owner_hwnd);
            }
            self.owner_hwnd = null_mut();
        }
    }
}
fn native_arabic_font(hwnd: HWND) -> HFONT {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let bytes = include_bytes!("../../assets/fonts/NotoSansArabic-Regular.ttf");
        let mut count = 0;
        // The OS keeps this private registration for the process lifetime. The
        // embedded bytes stay alive; no installed font or network is required.
        let handle = unsafe {
            AddFontMemResourceEx(
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                null_mut(),
                std::ptr::addr_of_mut!(count),
            )
        };
        if handle.is_null() {
            tracing::warn!("bundled Arabic font registration failed");
        }
    });
    unsafe {
        let default_font = SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT;
        let default_font = if default_font.is_null() {
            GetStockObject(DEFAULT_GUI_FONT)
        } else {
            default_font
        };
        let mut description: LOGFONTW = zeroed();
        if GetObjectW(
            default_font,
            std::mem::size_of::<LOGFONTW>() as i32,
            (&mut description as *mut LOGFONTW).cast(),
        ) == 0
        {
            return null_mut();
        }
        description.lfFaceName.fill(0);
        for (slot, character) in description
            .lfFaceName
            .iter_mut()
            .zip("Noto Sans Arabic".encode_utf16())
        {
            *slot = character;
        }
        CreateFontIndirectW(&description)
    }
}

impl Drop for TooltipState {
    fn drop(&mut self) {
        self.destroy_tooltip();
    }
}

fn monitor_work_area(point: POINT) -> RECT {
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            return info.rcWork;
        }
    }
    RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    }
}

fn cursor_inside(point: POINT, size: (i32, i32), cursor: POINT) -> bool {
    cursor.x >= point.x
        && cursor.x <= point.x + size.0
        && cursor.y >= point.y
        && cursor.y <= point.y + size.1
}

fn make_lparam(x: i32, y: i32) -> LPARAM {
    ((y as u32) << 16 | (x as u32 & 0xffff)) as LPARAM
}

fn tooltip_info_size() -> u32 {
    (std::mem::size_of::<TTTOOLINFOW>() - std::mem::size_of::<*mut std::ffi::c_void>()) as u32
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_tooltip_stays_visible_across_updates_and_reopens() {
        use super::*;
        use std::ptr::null;
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DefWindowProcW, IsWindowVisible, RegisterClassW, UnregisterClassW, WNDCLASSW,
        };
        let mut state = TooltipState::new();
        state.handle_command(WindowsCommand::Update(OverlayCommand::StartRecording(
            RecordingIndicators::default(),
        )));
        state.tick();
        let hwnd = state.hwnd;
        assert!(!hwnd.is_null());
        for level in [0.0, 0.5, 1.0, 0.3] {
            state.set_level(level);
            state.tick();
            pump_messages();
            assert_eq!(state.hwnd, hwnd);
            assert!(state.active);
            assert_ne!(unsafe { IsWindowVisible(hwnd) }, 0);
        }
        let instance = unsafe { GetModuleHandleW(null()) };
        let class = WNDCLASSW {
            hInstance: instance,
            lpszClassName: WINDOWS_PICKER_CLASS,
            lpfnWndProc: Some(DefWindowProcW),
            ..Default::default()
        };
        assert_ne!(unsafe { RegisterClassW(&class) }, 0);
        let picker = unsafe {
            CreateWindowExW(
                0,
                WINDOWS_PICKER_CLASS,
                null(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                instance,
                null(),
            )
        };
        assert!(!picker.is_null());
        state.tick();
        let suppressed = state.hwnd.is_null() && !state.active;
        unsafe {
            DestroyWindow(picker);
            UnregisterClassW(WINDOWS_PICKER_CLASS, instance);
        }
        assert!(suppressed);
        state.tick();
        assert_ne!(unsafe { IsWindowVisible(state.hwnd) }, 0);
        state.handle_command(WindowsCommand::Update(OverlayCommand::Hide));
    }

    #[test]
    fn held_shortcuts_preserve_recording_and_use_the_existing_reader_tooltip() {
        use super::{OverlayCommand, RecordingIndicators, TooltipState, WindowsCommand};
        use crate::capture::overlay::NoticeLevel;

        let mut capture = TooltipState::new();
        let shortcuts = "🔴 \u{2066}[CapsLock][S]\u{2069}";
        capture.handle_command(WindowsCommand::Update(OverlayCommand::SetShortcuts(
            shortcuts.into(),
            false,
        )));
        capture.shortcuts_held = true;
        assert!(capture.should_render());
        assert_eq!(capture.render_text(), shortcuts);

        let mut reader = TooltipState::new();
        reader.reader = true;
        reader.handle_command(WindowsCommand::Update(OverlayCommand::SetShortcuts(
            shortcuts.into(),
            false,
        )));
        reader.shortcuts_held = true;
        reader.handle_command(WindowsCommand::Update(OverlayCommand::Notify {
            level: NoticeLevel::Info,
            text: "Reading a passage.".into(),
            duration: None,
        }));
        reader.tick();
        assert!(reader.render_text().starts_with("Reading a passage.\n"));
        assert!(
            !capture.should_render(),
            "idle hint should use the visible reader tooltip"
        );

        capture.handle_command(WindowsCommand::Update(OverlayCommand::StartRecording(
            RecordingIndicators::default(),
        )));
        assert!(capture.should_render());
        assert!(capture.render_text().starts_with('🎙'));
        capture.shortcuts_held = false;
        assert!(capture.should_render());
        assert!(!capture.render_text().contains("[CapsLock]"));
        reader.shortcuts_held = false;
        assert_eq!(reader.render_text(), "Reading a passage.");
    }

    #[test]
    fn recording_waveform_is_prefix_free_unicode_and_stationary() {
        let mut levels = crate::capture::overlay::overlay_model::empty_visualizer_levels();
        crate::capture::overlay::overlay_model::set_visualizer_level(&mut levels, 0.4);
        let first = crate::capture::overlay::overlay_model::ascii_visualizer(&levels);
        let same = crate::capture::overlay::overlay_model::ascii_visualizer(&levels);
        crate::capture::overlay::overlay_model::set_visualizer_level(&mut levels, 0.8);
        let louder = crate::capture::overlay::overlay_model::ascii_visualizer(&levels);
        assert!(!first.to_ascii_lowercase().contains("rec"));
        assert_eq!(
            first.chars().count(),
            crate::capture::overlay::overlay_model::BAR_COUNT
        );
        assert_eq!(first, same);
        assert_ne!(first, louder);
        assert!(first.chars().all(|glyph| "▁▂▃▄▅▆▇".contains(glyph)));
    }
}
