//! Claude Code's folder trust. In a folder it has not been told to trust, an interactive session opens on "Is this a
//! project you trust?" and waits there, with nobody to answer. Before an interactive job runs, its folder is trusted
//! the way Claude Code's own messages describe: `projects[<folder>].hasTrustDialogAccepted` in `~/.claude.json`.

use crate::paths;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

/// Claude Code takes this lock (a folder, `proper-lockfile`) while it rewrites its config; it counts as stale after 10 s.
const LOCK_STALE: Duration = Duration::from_secs(10);

pub fn config_file() -> PathBuf {
    paths::home().join(".claude.json")
}

/// Makes sure Claude Code trusts `dir`, so an interactive session there starts without asking.
pub fn ensure(dir: &str) -> Result<(), String> {
    ensure_in(&config_file(), Path::new(dir), &paths::home())
}

fn ensure_in(config_path: &Path, dir: &Path, home: &Path) -> Result<(), String> {
    let dir = fs::canonicalize(dir).map_err(|e| format!("cannot open the working folder {}: {e}", dir.display()))?;
    if is_trusted(&read(config_path)?, &dir) {
        return Ok(());
    }
    if fs::canonicalize(home).is_ok_and(|h| h == dir) {
        return Err("Claude Code does not trust your home folder and never remembers it does, so an interactive \
                    session there would wait on its trust question. Pick another working folder in Advanced."
            .to_string());
    }
    let _lock = Lock::take(config_path)?;
    // Read again under the lock: Claude Code may have rewritten the file meanwhile.
    let mut config = read(config_path)?;
    let key = dir.to_str().ok_or("the working folder's path is not UTF-8")?;
    let projects = config
        .as_object_mut()
        .ok_or_else(|| format!("{} is not a JSON object", config_path.display()))?
        .entry("projects")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| format!("projects in {} is not a JSON object", config_path.display()))?;
    match projects.entry(key).or_insert_with(|| json!({})) {
        Value::Object(project) => {
            project.insert("hasTrustDialogAccepted".into(), Value::Bool(true));
        }
        other => *other = json!({ "hasTrustDialogAccepted": true }),
    }
    write(config_path, &config)
}

/// Whether Claude Code trusts `dir`: it or a folder above it was trusted, though never above its git repository,
/// where Claude Code stops looking.
fn is_trusted(config: &Value, dir: &Path) -> bool {
    let Some(projects) = config.get("projects").and_then(Value::as_object) else { return false };
    for folder in dir.ancestors() {
        let project = folder.to_str().and_then(|key| projects.get(key));
        if project.and_then(|p| p.get("hasTrustDialogAccepted")).and_then(Value::as_bool) == Some(true) {
            return true;
        }
        if folder.join(".git").exists() {
            return false;
        }
    }
    false
}

fn read(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}. Run claude once to set it up.", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{} does not parse: {e}", path.display()))
}

/// Replaces the file in one step, private to the user like the original.
fn write(path: &Path, config: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let tmp = path.with_file_name(".claude.json.claude-jobs.tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    file.write_all(text.as_bytes()).and_then(|()| file.sync_all()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
}

/// The lock Claude Code holds while it rewrites its config, so neither side loses the other's change.
struct Lock(PathBuf);

impl Lock {
    fn take(config_path: &Path) -> Result<Lock, String> {
        let path = PathBuf::from(format!("{}.lock", config_path.display()));
        for _ in 0..100 {
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Lock(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let age = fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| SystemTime::now().duration_since(t).ok());
                    if age.is_some_and(|age| age > LOCK_STALE) {
                        let _ = fs::remove_dir(&path);
                    } else {
                        thread::sleep(Duration::from_millis(100));
                    }
                }
                Err(e) => return Err(format!("cannot lock {}: {e}", config_path.display())),
            }
        }
        Err(format!("{} stayed locked by another Claude Code", config_path.display()))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("temp dir");
        let root = fs::canonicalize(tmp.path()).expect("canonical");
        fs::create_dir_all(root.join("home/jobs/daily")).expect("folders");
        (tmp, root)
    }

    fn trusting(folder: &Path) -> Value {
        json!({ "projects": { folder.to_str().expect("utf-8"): { "hasTrustDialogAccepted": true } } })
    }

    #[test]
    fn trust_reaches_down_to_subfolders_but_not_into_a_repository() {
        let (_tmp, root) = setup();
        let config = trusting(&root.join("home"));
        assert!(is_trusted(&config, &root.join("home/jobs/daily")));
        fs::create_dir(root.join("home/jobs/.git")).expect("repo");
        assert!(!is_trusted(&config, &root.join("home/jobs/daily")));
        assert!(is_trusted(&trusting(&root.join("home/jobs")), &root.join("home/jobs/daily")));
    }

    #[test]
    fn ensure_trusts_the_folder_and_keeps_the_rest_of_the_config() {
        let (_tmp, root) = setup();
        let config = root.join(".claude.json");
        fs::write(&config, r#"{"numStartups": 3, "projects": {"/elsewhere": {"allowedTools": []}}}"#).expect("write");
        ensure_in(&config, &root.join("home/jobs"), &root.join("home")).expect("trusted");
        let written = read(&config).expect("parses");
        let key = root.join("home/jobs").display().to_string();
        assert_eq!(written["projects"][&key]["hasTrustDialogAccepted"], true);
        assert_eq!(written["numStartups"], 3);
        assert_eq!(written["projects"]["/elsewhere"], json!({ "allowedTools": [] }));
        assert_eq!(fs::metadata(&config).expect("meta").permissions().mode() & 0o777, 0o600);
        assert!(!PathBuf::from(format!("{}.lock", config.display())).exists());
    }

    #[test]
    fn ensure_leaves_a_trusted_folder_alone_and_refuses_the_home_folder() {
        let (_tmp, root) = setup();
        let config = root.join(".claude.json");
        let text = serde_json::to_string(&trusting(&root.join("home/jobs"))).expect("json");
        fs::write(&config, &text).expect("write");
        ensure_in(&config, &root.join("home/jobs/daily"), &root.join("home")).expect("already trusted");
        assert_eq!(fs::read_to_string(&config).expect("read"), text);
        let error = ensure_in(&config, &root.join("home"), &root.join("home")).unwrap_err();
        assert!(error.contains("home folder"));
        assert_eq!(fs::read_to_string(&config).expect("read"), text);
    }
}
