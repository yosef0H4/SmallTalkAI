use super::{dispatch_media, Backend, MediaCommand};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

pub(super) const WORKSPACE_ENV: &str = "SMALLTALKAI_CLI_WORKSPACE";
const READY: &str = "ready";
const SPEECH_OUTPUT: &str = "speech.wav";

pub fn run_media(command: MediaCommand) -> Result<()> {
    if let MediaCommand::Speak {
        text,
        file,
        stdin,
        background: true,
        ..
    } = command
    {
        let text = super::speech_input(text, file, stdin)?;
        // The desktop Reader survives the lifetime of the caller's shell.
        crate::reader::read_in_background(&text)?;
        println!("Speech queued for background playback");
        return Ok(());
    }
    if let Some(workspace) = std::env::var_os(WORKSPACE_ENV) {
        let workspace = PathBuf::from(workspace);
        let started = Instant::now();
        // On Windows, wait until the parent has attached us to its job object.
        while !workspace.join(READY).is_file() {
            ensure!(
                started.elapsed() < Duration::from_secs(5),
                "media supervisor did not start"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        return dispatch_media(
            command,
            &mut OneShotBackend {
                workspace,
                transcriber: Default::default(),
            },
        );
    }

    // The child runs outside the foreground process group. Read interactive
    // stdin here so it cannot stop on SIGTTIN while waiting for text.
    let terminal_input = if matches!(&command, MediaCommand::Speak { stdin: true, .. })
        && std::io::stdin().is_terminal()
    {
        Some(super::read_limited(std::io::stdin(), "stdin")?)
    } else {
        None
    };
    let reader_data = crate::reader::data_dir();
    let profile = reader_data
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(profile)?;
    // Reader routing resolves installed assets relative to the profile's parent.
    let workspace = tempfile::Builder::new()
        .prefix(".media-")
        .tempdir_in(profile)?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let cancel = Arc::clone(&interrupted);
    ctrlc::set_handler(move || cancel.store(true, Ordering::Release))?;

    let mut process = Command::new(std::env::current_exe()?);
    process
        .args(std::env::args_os().skip(1))
        .env(WORKSPACE_ENV, workspace.path());
    if let Some(text) = terminal_input {
        let input = workspace.path().join("terminal-input.txt");
        fs::write(&input, text)?;
        process.stdin(fs::File::open(input)?);
    }
    let mut tree = MediaProcess::spawn(process)?;
    fs::write(workspace.path().join(READY), b"")?;
    let status = loop {
        if interrupted.load(Ordering::Acquire) {
            drop(tree);
            drop(workspace);
            std::process::exit(130);
        }
        if let Some(status) = tree.child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    drop(tree);
    if !status.success() {
        drop(workspace);
        std::process::exit(status.code().unwrap_or(1));
    }
    if let MediaCommand::Speak {
        output: Some(output),
        ..
    } = command
    {
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        std::io::copy(
            &mut fs::File::open(workspace.path().join(SPEECH_OUTPUT))?,
            &mut temporary,
        )?;
        ensure!(
            !interrupted.load(Ordering::Acquire),
            "media command interrupted"
        );
        temporary
            .persist_noclobber(&output)
            .with_context(|| format!("saving audio to {}", output.display()))?;
    }
    Ok(())
}

struct MediaProcess {
    child: Child,
    #[cfg(windows)]
    job: reader_core::windows_job::Job,
}

impl MediaProcess {
    fn spawn(mut command: Command) -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP);
        }
        let child = command.spawn().context("starting one-shot media process")?;
        #[cfg(windows)]
        let mut child = child;
        #[cfg(windows)]
        let job = reader_core::windows_job::Job::attach(&child).inspect_err(|_| {
            let _ = child.kill();
            let _ = child.wait();
        })?;
        Ok(Self {
            child,
            #[cfg(windows)]
            job,
        })
    }
}

impl Drop for MediaProcess {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return;
        }
        #[cfg(target_os = "linux")]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        self.job.terminate();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct OneShotBackend {
    workspace: PathBuf,
    transcriber: crate::capture_cli::Transcriber,
}

impl OneShotBackend {
    fn reader(&self, request: Value) -> Result<String> {
        let executable = std::env::current_exe()?;
        let sibling = executable
            .parent()
            .context("executable has no directory")?
            .join(format!(
                "smalltalkai-reader{}",
                std::env::consts::EXE_SUFFIX
            ));
        let mut child = Command::new(sibling)
            .arg("media")
            .arg("--config")
            .arg(crate::reader::store().path())
            .env("SMALLTALKAI_READER_DATA", &self.workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .context("starting private reading worker controller")?;
        let input = reader_core::wire::write_json_frame(
            &mut child.stdin.take().context("reader input unavailable")?,
            &request,
        );
        let output = child.wait_with_output()?;
        input?;
        ensure!(
            output.status.success(),
            "reading process exited with {}",
            output.status
        );
        let reply: Value = reader_core::wire::read_json_frame(&mut output.stdout.as_slice())?;
        let message = reply["message"]
            .as_str()
            .context("reading response has no message")?;
        ensure!(reply["ok"].as_bool() == Some(true), "{message}");
        Ok(message.to_owned())
    }
}

impl Backend for OneShotBackend {
    fn transcribe_wav(&mut self, wav: &Path, language: Option<&str>) -> Result<String> {
        self.transcriber.transcribe_owned(wav, language)
    }

    fn speak_text(&mut self, text: &str, output: Option<&Path>) -> Result<()> {
        let input = self.workspace.join("speech.txt");
        fs::write(&input, text)?;
        if output.is_some() {
            self.reader(
                json!({"op":"render","input":input,"output":self.workspace.join(SPEECH_OUTPUT)}),
            )?;
        } else {
            self.reader(json!({"op":"play","input":input}))?;
        }
        Ok(())
    }

    fn ocr_image(&mut self, image: &Path) -> Result<String> {
        self.reader(json!({"op":"ocr","path":image.canonicalize()?}))
    }
}
