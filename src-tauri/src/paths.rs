//! Where Claude Jobs keeps its files. `~/.claude-jobs` is shared with the earlier Python version, so its jobs and
//! run history carry over.

use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

pub fn base() -> PathBuf {
    home().join(".claude-jobs")
}

pub fn jobs_file() -> PathBuf {
    base().join("jobs.json")
}

pub fn runs_dir() -> PathBuf {
    base().join("runs")
}

pub fn logs_dir() -> PathBuf {
    base().join("logs")
}

pub fn agents_dir() -> PathBuf {
    home().join("Library").join("LaunchAgents")
}

pub fn expand_tilde(path: &str) -> PathBuf {
    match path.strip_prefix("~") {
        Some(rest) => home().join(rest.trim_start_matches('/')),
        None => PathBuf::from(path),
    }
}
