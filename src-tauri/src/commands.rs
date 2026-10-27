//! What the window can ask for. Every write goes through one lock, so two saves cannot interleave.

use crate::gcloud::{self, GcloudStatus};
use crate::jobs::{self, Defaults, Job};
use crate::runs::{self, RunDetail, RunRecord, Trigger};
use crate::{interactive, launchd, paths, runner, schedule};
use chrono::{Local, SecondsFormat};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use ts_rs::TS;

static WRITES: Mutex<()> = Mutex::new(());

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct JobView {
    #[serde(flatten)]
    pub job: Job,
    pub label: String,
    pub loaded: bool,
    pub next_run: Option<String>,
    pub last_run: Option<RunRecord>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct ApiError {
    pub errors: Vec<String>,
}

impl From<String> for ApiError {
    fn from(message: String) -> Self {
        ApiError { errors: vec![message] }
    }
}

impl From<Vec<String>> for ApiError {
    fn from(errors: Vec<String>) -> Self {
        ApiError { errors }
    }
}

type Reply<T> = Result<T, ApiError>;

fn exe() -> Reply<PathBuf> {
    std::env::current_exe().map_err(|e| ApiError::from(format!("cannot find the app's binary: {e}")))
}

fn view(job: Job) -> JobView {
    JobView {
        label: launchd::label(&job.id),
        loaded: launchd::is_loaded(&job.id),
        next_run: schedule::next_run(&job, Local::now()).map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, false)),
        last_run: runs::list(&job.id, 1).into_iter().next(),
        job,
    }
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    WRITES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[tauri::command(async)]
pub fn list_jobs() -> Reply<Vec<JobView>> {
    Ok(jobs::load()?.into_iter().map(view).collect())
}

#[tauri::command(async)]
pub fn defaults() -> Defaults {
    jobs::defaults()
}

#[tauri::command(async)]
pub fn save_job(job: Job, editing: Option<String>) -> Reply<JobView> {
    let _guard = lock();
    let mut all = jobs::load()?;
    let job = jobs::validate(job, &all, editing.as_deref())?;
    launchd::apply(&job, &exe()?)?;
    match all.iter_mut().find(|j| j.id == job.id) {
        Some(existing) => *existing = job.clone(),
        None => all.push(job.clone()),
    }
    jobs::save(&all)?;
    launchd::apply_catch_up(&all, &exe()?)?;
    Ok(view(job))
}

#[tauri::command(async)]
pub fn delete_job(id: String) -> Reply<()> {
    let _guard = lock();
    let all = jobs::load()?;
    if !all.iter().any(|j| j.id == id) {
        return Err(format!("No job called {id}.").into());
    }
    launchd::remove(&id);
    let rest: Vec<Job> = all.into_iter().filter(|j| j.id != id).collect();
    jobs::save(&rest)?;
    launchd::apply_catch_up(&rest, &exe()?)?;
    Ok(())
}

#[tauri::command(async)]
pub fn set_enabled(id: String, enabled: bool) -> Reply<JobView> {
    let _guard = lock();
    let mut all = jobs::load()?;
    let job = all.iter_mut().find(|j| j.id == id).ok_or_else(|| format!("No job called {id}."))?;
    job.enabled = enabled;
    let job = job.clone();
    launchd::apply(&job, &exe()?)?;
    jobs::save(&all)?;
    launchd::apply_catch_up(&all, &exe()?)?;
    Ok(view(job))
}

/// Starts a run now, detached from the window, with the environment launchd gives the job.
#[tauri::command(async)]
pub fn run_now(id: String) -> Reply<()> {
    let job = jobs::find(&id)?.ok_or_else(|| format!("No job called {id}."))?;
    if runs::is_running(&id) {
        return Err("This job is already running.".to_string().into());
    }
    runner::spawn(&job, Trigger::Manual)?;
    Ok(())
}

/// Opens Terminal on the live tmux session of an interactive run, to watch it or answer a question it is stuck on.
#[tauri::command(async)]
pub fn watch_run(id: String) -> Reply<()> {
    let job = jobs::find(&id)?.ok_or_else(|| format!("No job called {id}."))?;
    let tmux =
        interactive::tmux_bin(&job.path).ok_or_else(|| "tmux is not installed (brew install tmux).".to_string())?;
    let session = interactive::session_name(&id);
    if !interactive::has_session(&tmux, &session) {
        return Err("There is no live session to watch: the run has finished.".to_string().into());
    }
    let body = format!(
        "#!/bin/zsh\nexec {} -L {} attach -t {}\n",
        interactive::shell_quote(&tmux.display().to_string()),
        interactive::SOCKET,
        interactive::shell_quote(&session)
    );
    open_in_terminal(&format!("watch-{id}"), &body)?;
    Ok(())
}

/// A .command file opens in Terminal by itself, without asking to control Terminal.
fn open_in_terminal(name: &str, body: &str) -> Reply<()> {
    std::fs::create_dir_all(paths::logs_dir()).map_err(|e| e.to_string())?;
    let script = paths::logs_dir().join(format!("{name}.command"));
    std::fs::write(&script, body).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    Command::new("open").arg(&script).spawn().map_err(|e| format!("cannot open Terminal: {e}"))?;
    Ok(())
}

#[tauri::command(async)]
pub fn gcloud_status() -> GcloudStatus {
    gcloud::status()
}

/// Opens Terminal on `gcloud auth login`, which signs in through the browser.
#[tauri::command(async)]
pub fn gcloud_sign_in() -> Reply<()> {
    open_in_terminal("gcloud-sign-in", &gcloud::sign_in_script()?)
}

#[tauri::command(async)]
pub fn list_runs(id: String) -> Reply<Vec<RunRecord>> {
    if !jobs::valid_id(&id) {
        return Err("No such job.".to_string().into());
    }
    Ok(runs::list(&id, runs::KEEP_RUNS))
}

#[tauri::command(async)]
pub fn get_run(id: String, run_id: String) -> Reply<RunDetail> {
    Ok(runs::detail(&id, &run_id)?)
}

/// Points every enabled job's launchd file at this binary: after an update, a move, or on the first start after
/// the Python version.
pub fn sync_launchd() {
    let Ok(exe) = std::env::current_exe() else { return };
    let current = exe.display().to_string();
    let Ok(all) = jobs::load() else { return };
    for job in all.iter().filter(|j| j.enabled) {
        if launchd::program_of(&job.id).as_deref() != Some(current.as_str()) || !launchd::is_loaded(&job.id) {
            if let Err(e) = launchd::apply(job, &exe) {
                eprintln!("could not update the schedule of {}: {e}", job.id);
            }
        }
    }
    if let Err(e) = launchd::apply_catch_up(&all, &exe) {
        eprintln!("could not update the login catch-up: {e}");
    }
}
