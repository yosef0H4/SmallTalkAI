#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    let Err(error) = launch() else {
        return std::process::ExitCode::SUCCESS;
    };
    let title: Vec<u16> = "SmallTalkAI".encode_utf16().chain([0]).collect();
    let message: Vec<u16> = format!("Could not start SmallTalkAI.\n\n{error}")
        .encode_utf16()
        .chain([0])
        .collect();
    // Both strings are NUL-terminated and live for the duration of the call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
    std::process::ExitCode::FAILURE
}

#[cfg(windows)]
fn launch() -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

    let launcher = std::env::current_exe()?;
    let folder = launcher
        .parent()
        .ok_or_else(|| Error::other("Could not locate the launcher directory"))?;
    let runtime = folder.join("runtime");
    let interpreter = runtime.join("AutoHotkey64.exe");
    let script = runtime.join("smalltalkai.ahk");
    for path in [&interpreter, &script] {
        if !path.is_file() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Missing bundled file: {}", path.display()),
            ));
        }
    }
    Command::new(interpreter)
        .arg(script)
        .args(std::env::args_os().skip(1))
        .current_dir(&runtime)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This SmallTalkAI launcher is for Windows.");
    std::process::exit(1);
}
