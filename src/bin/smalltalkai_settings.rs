use anyhow::{Context, Result};
use clap::Parser;
use smalltalkai::settings_server::{self, Args};
use std::process::Command;

fn main() {
    if let Err(error) = run() {
        eprintln!("smalltalkai-settings: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    if !args.desktop() {
        return settings_server::run(args, None);
    }
    let executable = std::env::current_exe()?.with_file_name(format!(
        "smalltalkai-settings-window{}",
        std::env::consts::EXE_SUFFIX
    ));
    let mut command = Command::new(executable);
    command.args(std::env::args_os().skip(1));
    let error_context =
        "opening native Settings; pass --browser or --no-browser for browser Settings";
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec()).context(error_context)
    }
    #[cfg(not(unix))]
    {
        command.spawn().context(error_context)?;
        Ok(())
    }
}
