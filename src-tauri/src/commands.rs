//! What the window can ask for. Every write goes through one lock, so two saves cannot interleave.

use crate::conditions::{self, CommandCheck, CommandCondition, ConditionStatus, CLAUDE};
use crate::gcloud::{self, GcloudStatus};
use crate::jobs::{self, Defaults, Job};
use crate::runs::{self, RunDetail, RunRecord, Trigger, Waiting};
use crate::skills::{self, Skill};
use crate::tmux::{self, TmuxStatus};
use crate::update::{self, AppUpdate};
use crate::{interactive, launchd, paths, runner, schedule, trust};
use chrono::{Local, SecondsFormat};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use tauri::AppHandle;
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
    /// A run held until the job's conditions are met, which starts by itself once they are.
    pub waiting: Option<Waiting>,
    /// The conditions the job needs that are not met, from a check at most a few minutes old.
    pub unmet_conditions: Vec<ConditionStatus>,
}

/// A command condition, with its last check.
#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct CommandConditionView {
    #[serde(flatten)]
    pub condition: CommandCondition,
    /// Why it is not met; none when it is.
    pub problem: Option<String>,
    pub checked_at: String,
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
        waiting: runs::waiting(&job.id),
        unmet_conditions: conditions::unmet(&job, false),
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
    let commands = conditions::load_commands()?;
    let unknown: Vec<&str> =
        job.conditions.iter().map(String::as_str).filter(|c| !conditions::is_known(c, &commands)).collect();
    if !unknown.is_empty() {
        return Err(format!("No condition called {}.", unknown.join(", ")).into());
    }
    if job.interactive {
        // Here rather than at the first run, so a folder Claude Code cannot trust shows in the editor.
        trust::ensure(&job.cwd)?;
    }
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
    if runs::waiting(&id).is_some() {
        return Err("This job is already waiting for its conditions: it starts as soon as they are met."
            .to_string()
            .into());
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
    if interactive::attached_ttys(&tmux, &session).iter().any(|tty| focus_terminal_tab(tty)) {
        return Ok(());
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

/// Brings forward the Terminal tab on `tty`. The first time, macOS asks to let Claude Jobs control Terminal; without
/// that permission this returns false and Watch opens a new window as before.
fn focus_terminal_tab(tty: &str) -> bool {
    const SCRIPT: &str = r#"on run argv
    set wanted to item 1 of argv
    tell application "Terminal"
        repeat with w in windows
            repeat with t in tabs of w
                if tty of t is wanted then
                    set selected of t to true
                    set index of w to 1
                    activate
                    return "found"
                end if
            end repeat
        end repeat
    end tell
    return "missing"
end run"#;
    Command::new("osascript")
        .args(["-e", SCRIPT, tty])
        .output()
        .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "found")
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
    let status = gcloud::status();
    conditions::record_gcloud(&status);
    status
}

/// Signs in to Google Cloud through the browser and returns the new status once done.
#[tauri::command(async)]
pub fn gcloud_sign_in() -> Reply<GcloudStatus> {
    let status = gcloud::sign_in()?;
    conditions::record_gcloud(&status);
    Ok(status)
}

/// The job whose Claude binary and PATH the window checks Claude Code with: the jobs normally share them.
fn claude_job() -> Reply<Option<Job>> {
    Ok(jobs::load()?.into_iter().next())
}

/// Whether Claude Code is signed in, checked now, as the runs would find it. Met while there is no job to run.
#[tauri::command(async)]
pub fn claude_status() -> Reply<ConditionStatus> {
    Ok(match claude_job()? {
        Some(job) => conditions::claude_now(&job),
        None => ConditionStatus { condition: CLAUDE.to_string(), problem: None },
    })
}

/// The PATH a command condition is checked with outside a run: the jobs', which they normally share.
fn check_path() -> Reply<String> {
    Ok(claude_job()?.map(|job| job.path).unwrap_or_else(|| jobs::defaults().path))
}

/// The command conditions, each with its last check: a recent one, or one made now.
#[tauri::command(async)]
pub fn list_conditions() -> Reply<Vec<CommandConditionView>> {
    let path = check_path()?;
    Ok(conditions::load_commands()?
        .into_iter()
        .map(|condition| {
            let (problem, checked_at) = conditions::last_check(&condition.id).unwrap_or_else(|| {
                let (found, at) = conditions::check_command(&condition, &path, true);
                (found.problem, at)
            });
            CommandConditionView { condition, problem, checked_at }
        })
        .collect())
}

#[tauri::command(async)]
pub fn save_condition(condition: CommandCondition, editing: Option<String>) -> Reply<CommandCondition> {
    let _guard = lock();
    let mut all = conditions::load_commands()?;
    let condition = conditions::validate_command(condition, &all, editing.as_deref())?;
    match all.iter_mut().find(|c| c.id == condition.id) {
        Some(existing) => *existing = condition.clone(),
        None => all.push(condition.clone()),
    }
    conditions::save_commands(&all)?;
    // Checked now, so the window and the jobs show it at once, rather than an earlier version's check.
    conditions::check_command(&condition, &check_path()?, true);
    Ok(condition)
}

/// Deletes a command condition, and takes it off the jobs that ticked it.
#[tauri::command(async)]
pub fn delete_condition(id: String) -> Reply<()> {
    let _guard = lock();
    let all = conditions::load_commands()?;
    if !all.iter().any(|c| c.id == id) {
        return Err(format!("No condition called {id}.").into());
    }
    let mut jobs = jobs::load()?;
    if jobs.iter().any(|j| j.conditions.contains(&id)) {
        for job in &mut jobs {
            job.conditions.retain(|c| *c != id);
        }
        jobs::save(&jobs)?;
    }
    conditions::save_commands(&all.into_iter().filter(|c| c.id != id).collect::<Vec<_>>())?;
    Ok(())
}

/// Runs a command condition now, saved or not, and says what its command did. A saved one keeps the check.
#[tauri::command(async)]
pub fn check_condition(condition: CommandCondition) -> Reply<CommandCheck> {
    let saved = conditions::load_commands()?.contains(&condition);
    Ok(conditions::check_command(&condition, &check_path()?, saved).0)
}

/// Opens Terminal on `claude auth login`, with the environment the runs get, so the login is the one they use.
#[tauri::command(async)]
pub fn claude_sign_in() -> Reply<()> {
    let (claude, path) = match claude_job()? {
        Some(job) => (job.claude, job.path),
        None => {
            let d = jobs::defaults();
            (d.claude, d.path)
        }
    };
    let home = paths::home().display().to_string();
    let user = std::env::var("USER").unwrap_or_default();
    let env = [("PATH", path.as_str()), ("HOME", home.as_str()), ("USER", user.as_str())]
        .map(|(name, value)| format!("{name}={}", interactive::shell_quote(value)))
        .join(" ");
    let body = format!(
        "#!/bin/zsh\nexec env -i {env} TERM=xterm-256color LANG=en_US.UTF-8 {} auth login\n",
        interactive::shell_quote(&claude)
    );
    open_in_terminal("claude-login", &body)
}

/// Whether tmux, which interactive jobs need, is found on `path`, the job's PATH, and Homebrew can install it.
#[tauri::command(async)]
pub fn tmux_status(path: String) -> TmuxStatus {
    tmux::status(&path)
}

/// Installs tmux with Homebrew and returns the new status once done.
#[tauri::command(async)]
pub fn install_tmux(path: String) -> Reply<TmuxStatus> {
    Ok(tmux::install(&path)?)
}

/// The skills a prompt can start with, as the job's runs would find them: its Claude binary, PATH and folder.
#[tauri::command(async)]
pub fn list_skills(claude: String, path: String, cwd: String) -> Reply<Vec<Skill>> {
    Ok(skills::list(&claude, &path, &cwd)?)
}

/// The version this app was built as, from package.json.
#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Reply<Option<AppUpdate>> {
    Ok(update::check(&app).await?)
}

/// Installs the newest release and restarts the app on it; only returns when that failed.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Reply<()> {
    Ok(update::install(&app).await?)
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
