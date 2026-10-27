//! Google Cloud sign-in. Jobs that read BigQuery or Cloud Logging fail once gcloud's credentials expire, so the app
//! checks them and runs `gcloud auth login` itself, which signs in through the browser without a terminal.

use crate::{jobs, paths};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use ts_rs::TS;

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
/// Long enough to pick an account and type a password; gcloud waits on its local redirect until then.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

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

/// Runs gcloud; its trimmed stdout when it succeeds in time, otherwise the end of its stderr, or `None` on timeout.
fn gcloud(bin: &Path, path: &str, args: &[&str], limit: Duration) -> Option<Result<String, String>> {
    let mut child = Command::new(bin)
        .args(args)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if child.try_wait().ok()?.is_some() {
            let out = child.wait_with_output().ok()?;
            if out.status.success() {
                return Some(Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()));
            }
            let err = String::from_utf8_lossy(&out.stderr);
            let tail: Vec<&str> = err.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            return Some(Err(tail[tail.len().saturating_sub(3)..].join(" ")));
        }
        thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

pub fn status() -> GcloudStatus {
    let path = search_path();
    let Some(bin) = find(&path) else { return GcloudStatus { installed: false, signed_in: false, account: None } };
    // Printing a token refreshes it, so this fails exactly when a run would ("Reauthentication failed").
    let ok = |args: &[&str]| gcloud(&bin, &path, args, CHECK_TIMEOUT).and_then(Result::ok).filter(|o| !o.is_empty());
    let signed_in = ok(&["auth", "print-access-token", "--quiet"]).is_some();
    let account = ok(&["config", "get-value", "account", "--quiet"]);
    GcloudStatus { installed: true, signed_in, account }
}

/// Signs in through the browser, the gcloud CLI and the application default credentials the client libraries use.
/// Returns once the sign-in is done, failed, or was left unfinished.
pub fn sign_in() -> Result<GcloudStatus, String> {
    let path = search_path();
    let bin = find(&path).ok_or("gcloud is not installed.")?;
    let args = ["auth", "login", "--update-adc", "--quiet"];
    match gcloud(&bin, &path, &args, SIGN_IN_TIMEOUT) {
        Some(Ok(_)) => Ok(status()),
        Some(Err(e)) => Err(format!("Google Cloud sign-in failed: {e}")),
        None => Err(format!(
            "The Google Cloud sign-in was not finished within {} minutes. Try again.",
            SIGN_IN_TIMEOUT.as_secs() / 60
        )),
    }
}
