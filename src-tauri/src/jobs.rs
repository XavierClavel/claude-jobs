//! Jobs: a prompt, when to run it and how. Stored in `~/.claude-jobs/jobs.json`.

use crate::paths;
use serde::{Deserialize, Serialize};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::{fs, thread};
use ts_rs::TS;

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub hour: u8,
    pub minute: u8,
    /// ISO weekdays, 1 = Monday to 7 = Sunday. Empty means every day.
    #[serde(default)]
    pub weekdays: Vec<u8>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_timeout")]
    pub timeout_minutes: u32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub claude: String,
    #[serde(default)]
    pub path: String,
}

fn default_timeout() -> u32 {
    30
}

fn default_enabled() -> bool {
    true
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct Defaults {
    pub claude: String,
    pub path: String,
    pub cwd: String,
}

pub fn load() -> Result<Vec<Job>, String> {
    match fs::read_to_string(paths::jobs_file()) {
        // A file that does not parse is an error, never "no jobs": saving over it would lose them.
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("jobs.json does not parse: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(format!("cannot read jobs.json: {e}")),
    }
}

pub fn save(jobs: &[Job]) -> Result<(), String> {
    fs::create_dir_all(paths::base()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(jobs).map_err(|e| e.to_string())?;
    let tmp = paths::jobs_file().with_extension("tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, paths::jobs_file()).map_err(|e| e.to_string())
}

pub fn find(id: &str) -> Result<Option<Job>, String> {
    Ok(load()?.into_iter().find(|j| j.id == id))
}

pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit())
        && id.len() <= 40
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Wrappers such as Superset's pick an account from their own environment, which a scheduled run does not have.
fn is_wrapper_dir(dir: &Path) -> bool {
    dir.starts_with(paths::home().join(".superset"))
}

/// The user's PATH as a login shell sees it. An app started from the Finder only gets `/usr/bin:/bin:…`.
fn login_shell_path() -> Option<String> {
    let mut child = Command::new("/bin/zsh")
        .args(["-ilc", "printf '\\n%s' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    for _ in 0..50 {
        if child.try_wait().ok()?.is_some() {
            let out = child.wait_with_output().ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            return text.lines().last().map(str::to_string).filter(|p| !p.is_empty());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    None
}

pub fn defaults() -> Defaults {
    let mut dirs: Vec<String> = Vec::new();
    let shell = login_shell_path().unwrap_or_default();
    let home = paths::home();
    let known = [
        home.join(".local/bin").display().to_string(),
        "/opt/homebrew/bin".into(),
        "/opt/homebrew/share/google-cloud-sdk/bin".into(),
        home.join("google-cloud-sdk/bin").display().to_string(),
        "/usr/local/bin".into(),
        "/usr/bin".into(),
        "/bin".into(),
        "/usr/sbin".into(),
        "/sbin".into(),
    ];
    for dir in shell.split(':').map(str::to_string).chain(known) {
        if !dir.is_empty() && !dirs.contains(&dir) && !is_wrapper_dir(Path::new(&dir)) && Path::new(&dir).is_dir() {
            dirs.push(dir);
        }
    }
    let claude = dirs
        .iter()
        .map(|d| Path::new(d).join("claude"))
        .find(|p| is_executable(p))
        .unwrap_or_else(|| home.join(".local/bin/claude"));
    Defaults { claude: claude.display().to_string(), path: dirs.join(":"), cwd: home.display().to_string() }
}

/// Checks a job from the editor and fills its defaults. `editing` is the id of the job being changed, if any.
pub fn validate(input: Job, existing: &[Job], editing: Option<&str>) -> Result<Job, Vec<String>> {
    let mut errors = Vec::new();
    let mut job = Job {
        id: input.id.trim().to_string(),
        name: input.name.trim().to_string(),
        prompt: input.prompt.trim().to_string(),
        allowed_tools: input.allowed_tools.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect(),
        model: input.model.trim().to_string(),
        ..input
    };
    if !valid_id(&job.id) {
        errors.push("Id: lowercase letters, digits and dashes, 40 characters at most.".to_string());
    }
    match editing {
        Some(current) if current != job.id => errors.push("A job's id cannot change.".to_string()),
        None if existing.iter().any(|j| j.id == job.id) => {
            errors.push(format!("A job called {} already exists.", job.id))
        }
        _ => {}
    }
    if job.name.is_empty() {
        errors.push("Name is required.".to_string());
    }
    if job.prompt.is_empty() {
        errors.push("Prompt is required.".to_string());
    }
    if job.hour > 23 || job.minute > 59 {
        errors.push("Time must be between 00:00 and 23:59.".to_string());
    }
    job.weekdays.sort_unstable();
    job.weekdays.dedup();
    if job.weekdays.iter().any(|d| !(1..=7).contains(d)) {
        errors.push("Days must be 1 (Monday) to 7 (Sunday).".to_string());
    }
    if !(1..=240).contains(&job.timeout_minutes) {
        errors.push("Timeout must be 1 to 240 minutes.".to_string());
    }
    let cwd = if job.cwd.trim().is_empty() { paths::home() } else { paths::expand_tilde(job.cwd.trim()) };
    if !cwd.is_dir() {
        errors.push(format!("Working folder {} does not exist.", cwd.display()));
    }
    job.cwd = cwd.display().to_string();
    let needs_defaults = job.claude.trim().is_empty() || job.path.trim().is_empty();
    let fallback = if needs_defaults { Some(defaults()) } else { None };
    job.claude = match &fallback {
        Some(d) if job.claude.trim().is_empty() => d.claude.clone(),
        _ => paths::expand_tilde(job.claude.trim()).display().to_string(),
    };
    if !is_executable(Path::new(&job.claude)) {
        errors.push(format!("Claude binary {} is not executable.", job.claude));
    }
    job.path = match &fallback {
        Some(d) if job.path.trim().is_empty() => d.path.clone(),
        _ => job.path.trim().to_string(),
    };
    if errors.is_empty() {
        Ok(job)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str) -> Job {
        Job {
            id: id.into(),
            name: "Test".into(),
            prompt: "/test".into(),
            hour: 9,
            minute: 0,
            weekdays: vec![],
            cwd: "/tmp".into(),
            allowed_tools: vec![" Read ".into(), "".into()],
            model: "".into(),
            timeout_minutes: 30,
            enabled: true,
            claude: "/bin/sh".into(),
            path: "/usr/bin:/bin".into(),
        }
    }

    #[test]
    fn ids_are_lowercase_slugs() {
        assert!(valid_id("photobook-monitor"));
        assert!(valid_id("a1"));
        assert!(!valid_id("-leading"));
        assert!(!valid_id("Upper"));
        assert!(!valid_id("has space"));
        assert!(!valid_id(&"a".repeat(41)));
    }

    #[test]
    fn validate_trims_tools_and_sorts_days() {
        let mut input = job("daily");
        input.weekdays = vec![5, 1, 5];
        let out = validate(input, &[], None).expect("valid");
        assert_eq!(out.allowed_tools, vec!["Read".to_string()]);
        assert_eq!(out.weekdays, vec![1, 5]);
    }

    #[test]
    fn validate_refuses_duplicates_bad_times_and_days() {
        let mut input = job("daily");
        input.hour = 24;
        input.weekdays = vec![0];
        let errors = validate(input, &[job("daily")], None).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("already exists")));
        assert!(errors.iter().any(|e| e.contains("Time")));
        assert!(errors.iter().any(|e| e.contains("Days")));
    }

    #[test]
    fn editing_keeps_the_id() {
        let errors = validate(job("other"), &[job("daily")], Some("daily")).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("cannot change")));
        assert!(validate(job("daily"), &[job("daily")], Some("daily")).is_ok());
    }
}
