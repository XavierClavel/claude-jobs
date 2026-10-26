//! What the window can ask for. Every write goes through one lock, so two saves cannot interleave.

use crate::jobs::{self, Defaults, Job};
use crate::runs::{self, RunDetail, RunRecord};
use crate::{launchd, paths, schedule};
use chrono::{Local, SecondsFormat};
use serde::Serialize;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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
    jobs::save(&all.into_iter().filter(|j| j.id != id).collect::<Vec<_>>())?;
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
    Ok(view(job))
}

/// Starts a run now, detached from the window, with the environment launchd gives the job.
#[tauri::command(async)]
pub fn run_now(id: String) -> Reply<()> {
    let job = jobs::find(&id)?.ok_or_else(|| format!("No job called {id}."))?;
    if runs::is_running(&id) {
        return Err("This job is already running.".to_string().into());
    }
    std::fs::create_dir_all(paths::logs_dir()).map_err(|e| e.to_string())?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::logs_dir().join(format!("{id}.launchd.log")))
        .map_err(|e| e.to_string())?;
    Command::new(exe()?)
        .args(["run", &id, "--trigger", "manual"])
        .env_clear()
        .env("PATH", &job.path)
        .env("HOME", paths::home())
        .env("USER", std::env::var("USER").unwrap_or_default())
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        // Keeps running if the window closes.
        .process_group(0)
        .spawn()
        .map_err(|e| format!("cannot start the run: {e}"))?;
    Ok(())
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
}
