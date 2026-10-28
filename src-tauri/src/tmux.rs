//! tmux, which interactive jobs run in. When it is missing, the editor installs it with Homebrew in the background,
//! so nobody has to open a terminal for it.

use crate::{interactive, paths};
use serde::Serialize;
use std::fs::{self, File};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use ts_rs::TS;

/// Homebrew may update itself first, then downloads tmux and its libraries.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct TmuxStatus {
    pub installed: bool,
    /// Homebrew is there to install it.
    pub can_install: bool,
}

/// Apple silicon's Homebrew, then Intel's.
fn brew() -> Option<PathBuf> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"].map(PathBuf::from).into_iter().find(|p| p.is_file())
}

/// `path` is the job's PATH, where its runs look for tmux.
pub fn status(path: &str) -> TmuxStatus {
    TmuxStatus { installed: interactive::tmux_bin(path).is_some(), can_install: brew().is_some() }
}

/// Runs `brew install tmux` and returns once it is done. What Homebrew printed is in `logs/tmux-install.log`.
pub fn install(path: &str) -> Result<TmuxStatus, String> {
    let brew = brew().ok_or("Homebrew is not installed: get it from brew.sh, or install tmux another way.")?;
    let bin_dir = brew.parent().map(|d| d.display().to_string()).unwrap_or_default();
    fs::create_dir_all(paths::logs_dir()).map_err(|e| e.to_string())?;
    let log_path = paths::logs_dir().join("tmux-install.log");
    let log = File::create(&log_path).map_err(|e| e.to_string())?;
    let mut child = Command::new(&brew)
        .args(["install", "tmux"])
        // An app opened from the Finder gets a bare PATH; Homebrew needs its own folder and the system tools.
        .env("PATH", format!("{bin_dir}:/usr/bin:/bin:/usr/sbin:/sbin"))
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .spawn()
        .map_err(|e| format!("cannot run Homebrew: {e}"))?;
    let deadline = Instant::now() + INSTALL_TIMEOUT;
    loop {
        if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
            if exit.success() {
                return Ok(status(path));
            }
            let text = fs::read_to_string(&log_path).unwrap_or_default();
            let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            return Err(format!("brew install tmux failed: {}", lines[lines.len().saturating_sub(3)..].join(" ")));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "brew install tmux did not finish within {} minutes. What it printed is in {}.",
                INSTALL_TIMEOUT.as_secs() / 60,
                log_path.display()
            ));
        }
        thread::sleep(Duration::from_millis(500));
    }
}
