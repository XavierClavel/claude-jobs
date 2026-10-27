//! Google Cloud sign-in. Jobs that read BigQuery or Cloud Logging fail once gcloud's credentials expire, and signing
//! in again needs a browser, so the app checks them and opens Terminal on `gcloud auth login`.

use crate::{jobs, paths};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use ts_rs::TS;

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct GcloudStatus {
    pub installed: bool,
    pub signed_in: bool,
    pub account: Option<String>,
}

/// The jobs' PATH first, since that is where their runs find gcloud, then the usual install folders.
pub fn search_path() -> String {
    let home = paths::home();
    let mut dirs: Vec<String> = Vec::new();
    let from_jobs = jobs::load().unwrap_or_default().into_iter().map(|j| j.path).collect::<Vec<_>>();
    let known = [
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/share/google-cloud-sdk/bin".to_string(),
        home.join("google-cloud-sdk/bin").display().to_string(),
        "/usr/local/bin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
    ];
    for dir in from_jobs.iter().flat_map(|p| p.split(':')).map(str::to_string).chain(known) {
        if !dir.is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs.join(":")
}

fn find(path: &str) -> Option<PathBuf> {
    path.split(':').map(|d| Path::new(d).join("gcloud")).find(|p| p.is_file())
}

/// Runs gcloud and returns its trimmed stdout when it succeeds in time.
fn gcloud(bin: &Path, path: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(bin)
        .args(args)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + CHECK_TIMEOUT;
    while Instant::now() < deadline {
        if child.try_wait().ok()?.is_some() {
            let out = child.wait_with_output().ok()?;
            return out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    None
}

pub fn status() -> GcloudStatus {
    let path = search_path();
    let Some(bin) = find(&path) else { return GcloudStatus { installed: false, signed_in: false, account: None } };
    // Printing a token refreshes it, so this fails exactly when a run would ("Reauthentication failed").
    let signed_in = gcloud(&bin, &path, &["auth", "print-access-token", "--quiet"]).is_some_and(|t| !t.is_empty());
    let account = gcloud(&bin, &path, &["config", "get-value", "account", "--quiet"]).filter(|a| !a.is_empty());
    GcloudStatus { installed: true, signed_in, account }
}

/// The Terminal script that signs in: the gcloud CLI and the application default credentials the client libraries use.
pub fn sign_in_script() -> Result<String, String> {
    let path = search_path();
    let bin = find(&path).ok_or("gcloud is not installed.")?;
    Ok(format!(
        "#!/bin/zsh\nexport PATH={}\n{} auth login --update-adc\necho\nread -k1 '?Press any key to close this window.'\n",
        crate::interactive::shell_quote(&path),
        crate::interactive::shell_quote(&bin.display().to_string())
    ))
}
