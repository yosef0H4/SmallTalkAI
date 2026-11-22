use anyhow::{bail, ensure, Context, Result};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const SKILL: &str = include_str!("../skills/smalltalkai/SKILL.md");
const OWNED: &str = "<!-- Installed by SmallTalkAI -->";
const PATH_START: &str = "<!-- executable -->";
const PATH_END: &str = "<!-- /executable -->";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    Codex,
    ClaudeCode,
    Cursor,
    Opencode,
}

#[derive(Serialize)]
pub struct InstalledSkill {
    path: PathBuf,
    executable: PathBuf,
}

#[derive(Serialize)]
pub struct AgentInfo {
    id: Agent,
    name: &'static str,
    detected: bool,
    installed: bool,
}

impl Agent {
    fn locations(self, home: &Path) -> (&'static str, &'static str, PathBuf, PathBuf) {
        match self {
            Self::Codex => (
                "Codex",
                "codex",
                home.join(".codex"),
                home.join(".agents/skills"),
            ),
            Self::ClaudeCode => {
                let config = std::env::var_os("CLAUDE_CONFIG_DIR")
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".claude"));
                (
                    "Claude Code",
                    "claude",
                    config.clone(),
                    config.join("skills"),
                )
            }
            Self::Cursor => (
                "Cursor",
                "cursor",
                home.join(".cursor"),
                home.join(".cursor/skills"),
            ),
            Self::Opencode => {
                let config = std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .unwrap_or_else(|| home.join(".config"))
                    .join("opencode");
                (
                    "OpenCode",
                    "opencode",
                    config.clone(),
                    config.join("skills"),
                )
            }
        }
    }
}

pub fn list() -> Result<Vec<AgentInfo>> {
    let home = dirs::home_dir().context("user home directory is unavailable")?;
    let paths = std::env::var_os("PATH").unwrap_or_default();
    Ok(Agent::value_variants()
        .iter()
        .map(|&id| {
            let (name, command, config, destination) = id.locations(&home);
            let found_on_path = std::env::split_paths(&paths).any(|dir| {
                if cfg!(windows) {
                    ["exe", "cmd", "bat"]
                        .iter()
                        .any(|suffix| dir.join(format!("{command}.{suffix}")).is_file())
                } else {
                    dir.join(command).is_file()
                }
            });
            let default_cursor_install = cfg!(windows)
                && matches!(id, Agent::Cursor)
                && std::env::var_os("LOCALAPPDATA").is_some_and(|path| {
                    PathBuf::from(path)
                        .join("Programs/cursor/Cursor.exe")
                        .is_file()
                });
            AgentInfo {
                id,
                name,
                detected: config.is_dir() || found_on_path || default_cursor_install,
                installed: destination.join("smalltalkai/SKILL.md").is_file(),
            }
        })
        .collect())
}

pub fn install(agent: Agent, project: bool) -> Result<InstalledSkill> {
    let home = dirs::home_dir().context("user home directory is unavailable")?;
    let executable = std::env::current_exe()?
        .with_file_name(format!("smalltalkai-ctl{}", std::env::consts::EXE_SUFFIX));
    ensure!(
        executable.is_file(),
        "SmallTalkAI CLI was not found at {}",
        executable.display()
    );
    let destination = if project {
        std::env::current_dir()?.join(match agent {
            Agent::Codex => ".agents/skills",
            Agent::ClaudeCode => ".claude/skills",
            Agent::Cursor => ".cursor/skills",
            Agent::Opencode => ".opencode/skills",
        })
    } else {
        agent.locations(&home).3
    };
    let folder = destination.join("smalltalkai");
    let path = folder.join("SKILL.md");
    let content = render(&executable)?;
    let existing = match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file(),
                "existing skill is not a regular file: {}",
                path.display()
            );
            let current = fs::read_to_string(&path)?;
            if current == content {
                return Ok(InstalledSkill { path, executable });
            }
            ensure!(
                current.lines().any(|line| line == OWNED),
                "a custom SmallTalkAI skill already exists at {}",
                path.display()
            );
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error).with_context(|| format!("checking {}", path.display())),
    };
    fs::create_dir_all(&folder)?;
    let mut temporary = tempfile::NamedTempFile::new_in(folder)?;
    temporary.write_all(content.as_bytes())?;
    if existing {
        temporary.persist(&path)?;
    } else {
        temporary.persist_noclobber(&path)?;
    }
    Ok(InstalledSkill { path, executable })
}

fn render(executable: &Path) -> Result<String> {
    let path = executable.to_str().context("CLI path is not valid UTF-8")?;
    ensure!(
        !path.chars().any(char::is_control) && !path.contains("```"),
        "CLI path cannot be represented in the skill"
    );
    let Some((before, executable_section)) = SKILL.split_once(PATH_START) else {
        bail!("bundled skill has no executable section");
    };
    let (_, after) = executable_section
        .split_once(PATH_END)
        .context("bundled skill is incomplete")?;
    Ok(format!("{before}{OWNED}\n{PATH_START}\nUse this absolute executable path:\n\n```text\n{path}\n```\n{PATH_END}{after}"))
}
