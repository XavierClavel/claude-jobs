//! One headless run of a job: `claude -p <prompt> --output-format json`, recorded in the job's history.

use crate::conditions::{self, ConditionStatus, CLAUDE_EXPIRED};
use crate::jobs::{self, Job};
use crate::runs::{self, RunRecord, RunStatus, Trigger, Waiting};
use crate::{paths, schedule};
use chrono::{DateTime, Local, SecondsFormat, Utc};
use std::fs::File;
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const NETWORK_WAIT: Duration = Duration::from_secs(300);
/// How often a run waiting for its job's conditions checks them again.
const CONDITION_POLL: Duration = Duration::from_secs(30);
/// A wait shorter than this, such as for the network after the Mac wakes, needs no alert.
const WAIT_ALERT_AFTER: Duration = Duration::from_secs(120);

/// A session that finishes is not a run that worked: Claude says which it was on the last line of its answer.
pub const STATUS_INSTRUCTION: &str = "This is an unattended scheduled run that nobody watches. End your final \
message with a line of its own: `RUN STATUS: FAILED` when the task could not be done (for example a data source, \
credential or tool failed and you stopped), otherwise `RUN STATUS: OK`.";
const STATUS_PREFIX: &str = "RUN STATUS:";

/// How Claude Code turns away a session whose login expired: "Login expired · Please run /login".
pub fn is_login_error(text: &str) -> bool {
    text.contains("Login expired") || text.contains("Please run /login")
}

/// How a run that started ended.
pub enum Outcome {
    /// Done, well or not: the record says how.
    Finished,
    /// Claude Code turned the run away before it did anything: its login expired.
    LoginExpired,
}

/// Removes the status line from Claude's answer and returns whether it reported a failure.
fn split_status(result: &str) -> (String, bool) {
    let trimmed = result.trim_end();
    let (body, last) = trimmed.rsplit_once('\n').unwrap_or(("", trimmed));
    let last = last.trim().trim_matches('`').trim();
    match last.strip_prefix(STATUS_PREFIX) {
        Some(verdict) => (body.trim_end().to_string(), verdict.trim().eq_ignore_ascii_case("failed")),
        None => (result.to_string(), false),
    }
}

/// Turns a finished run Claude reported as failed into a failed run, its first line as the error.
pub fn apply_verdict(rec: &mut RunRecord) {
    let Some(result) = rec.result.take() else { return };
    let (body, failed) = split_status(&result);
    if failed && rec.status == RunStatus::Ok {
        rec.status = RunStatus::Failed;
        let first = body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Claude reported a failure.");
        rec.error = Some(first.to_string());
    }
    rec.result = Some(body);
}

/// Exit codes for launchd's log: 0 ok, 1 the run failed, 2 no such job, 3 already running.
pub fn run_job(job_id: &str, trigger: Trigger) -> i32 {
    let job = match jobs::find(job_id) {
        Ok(Some(job)) => job,
        Ok(None) => {
            eprintln!("no job called {job_id}");
            return 2;
        }
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if runs::is_running(job_id) || runs::waiting(job_id).is_some() {
        eprintln!("{job_id} is already running");
        return 3;
    }
    if trigger == Trigger::Schedule && job.last_day_fallback {
        let last = runs::list(job_id, 1).into_iter().next();
        let last_started = last.and_then(|r| DateTime::parse_from_rfc3339(&r.started_at).ok());
        // 28 February of a leap year, which launchd cannot tell apart from the last day of other Februaries.
        if schedule::already_ran(&job, Local::now(), last_started) {
            eprintln!("{job_id}: its last scheduled time has already run");
            return 0;
        }
    }
    let mut job = job;
    loop {
        job = match wait_for_conditions(job, trigger) {
            Some(job) => job,
            None => return 0,
        };
        if let Some(code) = attempt(&job, trigger) {
            return code;
        }
        eprintln!("{job_id}: Claude Code's login expired; the run waits for a new one");
    }
}

/// Holds the run until the job's conditions are met, checking every 30 s, and returns the job as it is then. None
/// when the job was deleted meanwhile, or paused while its scheduled run waited.
fn wait_for_conditions(job: Job, trigger: Trigger) -> Option<Job> {
    let mut unmet = conditions::unmet(&job, true);
    if unmet.is_empty() {
        return Some(job);
    }
    let waiting =
        Waiting { since: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true), trigger, pid: std::process::id() };
    if let Err(e) = runs::save_waiting(&job.id, &waiting) {
        eprintln!("cannot record the wait: {e}");
    }
    eprintln!("{}: waiting: {}", job.id, problems(&unmet));
    let was_enabled = job.enabled;
    let since = Instant::now();
    // Whoever started a run by hand is looking at it.
    let mut alerted = trigger == Trigger::Manual;
    let mut job = job;
    loop {
        if !alerted && since.elapsed() >= WAIT_ALERT_AFTER {
            let message = format!("{} The job starts as soon as that is done.", problems(&unmet));
            alert(&format!("Claude job waiting: {}", job.name), &message);
            alerted = true;
        }
        thread::sleep(CONDITION_POLL);
        match jobs::find(&job.id) {
            Ok(Some(current)) if current.enabled || !was_enabled => job = current,
            Ok(_) => {
                runs::clear_waiting(&job.id);
                eprintln!("{}: paused or deleted, no longer waiting", job.id);
                return None;
            }
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        }
        unmet = conditions::unmet(&job, true);
        if unmet.is_empty() {
            break;
        }
    }
    runs::clear_waiting(&job.id);
    eprintln!("{}: conditions met after {} s, starting", job.id, since.elapsed().as_secs());
    Some(job)
}

fn problems(unmet: &[ConditionStatus]) -> String {
    unmet.iter().filter_map(|c| c.problem.as_deref()).collect::<Vec<_>>().join(" ")
}

/// One recorded run, and its exit code. None when Claude Code turned it away for an expired login: the run is
/// forgotten, and waits for a new login.
fn attempt(job: &Job, trigger: Trigger) -> Option<i32> {
    let job_id = job.id.as_str();
    let started = Utc::now();
    let mut rec = RunRecord {
        id: started.format("%Y%m%dT%H%M%SZ").to_string(),
        job: job.id.clone(),
        trigger,
        status: RunStatus::Running,
        started_at: started.to_rfc3339_opts(SecondsFormat::Secs, true),
        ended_at: None,
        duration_s: None,
        exit_code: None,
        cost_usd: None,
        num_turns: None,
        session_id: None,
        result: None,
        error: None,
        pid: Some(std::process::id()),
        claude_pid: None,
        prompt: Some(job.prompt.clone()),
        interactive: job.interactive,
        tmux_session: None,
    };
    if let Err(e) = runs::save(&rec) {
        eprintln!("cannot record the run: {e}");
        return Some(1);
    }

    let outcome = if job.interactive { crate::interactive::execute(job, &mut rec) } else { execute(job, &mut rec) };
    let ended = Utc::now();
    rec.ended_at = Some(ended.to_rfc3339_opts(SecondsFormat::Secs, true));
    rec.duration_s = Some((ended - started).num_seconds().max(0) as u32);
    match outcome {
        Ok(Outcome::Finished) => {}
        Ok(Outcome::LoginExpired) if conditions::record_expired_login() => {
            runs::remove(job_id, &rec.id);
            return None;
        }
        Ok(Outcome::LoginExpired) => {
            rec.status = RunStatus::Failed;
            rec.error = Some(CLAUDE_EXPIRED.to_string());
        }
        Err(e) => {
            rec.status = RunStatus::Failed;
            rec.error = Some(e);
        }
    }
    let _ = runs::save(&rec);
    runs::prune(job_id);
    if rec.status != RunStatus::Ok {
        let last_line = rec.error.as_deref().and_then(|e| e.lines().last()).unwrap_or("The run did not finish.");
        alert(&format!("Claude job failed: {}", job.name), last_line);
    }
    Some(if rec.status == RunStatus::Ok { 0 } else { 1 })
}

/// Starts `run <job>` in its own process, detached from the caller, with the environment launchd gives the job.
pub fn spawn(job: &Job, trigger: Trigger) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find the app's binary: {e}"))?;
    std::fs::create_dir_all(paths::logs_dir()).map_err(|e| e.to_string())?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::logs_dir().join(format!("{}.launchd.log", job.id)))
        .map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.args(["run", &job.id]);
    match trigger {
        Trigger::Schedule => {}
        Trigger::Manual => {
            cmd.args(["--trigger", "manual"]);
        }
        Trigger::CatchUp => {
            cmd.args(["--trigger", "catch-up"]);
        }
    }
    cmd.env_clear()
        .env("PATH", &job.path)
        .env("HOME", paths::home())
        .env("USER", std::env::var("USER").unwrap_or_default())
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        // Keeps running if the caller exits.
        .process_group(0)
        .spawn()
        .map_err(|e| format!("cannot start the run: {e}"))?;
    Ok(())
}

/// What the login agent calls: starts every job whose last scheduled time passed while the Mac was off.
pub fn catch_up() -> i32 {
    let all = match jobs::load() {
        Ok(all) => all,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let now = Local::now();
    let due: Vec<&Job> = all
        .iter()
        .filter(|job| {
            let last = runs::list(&job.id, 1).into_iter().next();
            let started = last.and_then(|r| DateTime::parse_from_rfc3339(&r.started_at).ok());
            schedule::missed_run(job, now, started).is_some()
        })
        .collect();
    if due.is_empty() {
        return 0;
    }
    wait_for_network();
    let mut code = 0;
    for job in due {
        eprintln!("{}: catching up the run missed while the Mac was off", job.id);
        if let Err(e) = spawn(job, Trigger::CatchUp) {
            eprintln!("{}: {e}", job.id);
            code = 1;
        }
    }
    code
}

/// Right after login Wi-Fi may not be up yet, and a run would fail on its first request.
fn wait_for_network() {
    let deadline = Instant::now() + NETWORK_WAIT;
    while Instant::now() < deadline {
        let reachable = ("api.anthropic.com", 443)
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| addrs.next())
            .is_some_and(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(5)).is_ok());
        if reachable {
            return;
        }
        thread::sleep(Duration::from_secs(5));
    }
    eprintln!("no network after {} s, starting anyway", NETWORK_WAIT.as_secs());
}

fn execute(job: &Job, rec: &mut RunRecord) -> Result<Outcome, String> {
    let dir = runs::dir(&job.id);
    let out_path = dir.join(format!("{}.out", rec.id));
    let err_path = dir.join(format!("{}.err", rec.id));
    let mut cmd = Command::new(&job.claude);
    cmd.args(["-p", &job.prompt, "--output-format", "json", "--append-system-prompt", STATUS_INSTRUCTION]);
    if !job.model.is_empty() {
        cmd.args(["--model", &job.model]);
    }
    if job.chrome {
        cmd.arg("--chrome");
    }
    if !job.allowed_tools.is_empty() {
        cmd.arg("--allowedTools").args(&job.allowed_tools);
    }
    cmd.current_dir(&job.cwd)
        .env_clear()
        .env("PATH", &job.path)
        .env("HOME", paths::home())
        .env("USER", std::env::var("USER").unwrap_or_default())
        .env("SHELL", "/bin/zsh")
        .env("LANG", "en_US.UTF-8")
        .stdin(Stdio::null())
        .stdout(File::create(&out_path).map_err(|e| e.to_string())?)
        .stderr(File::create(&err_path).map_err(|e| e.to_string())?)
        // Its own process group, so a timeout stops Claude and everything it started.
        .process_group(0);
    let mut child = cmd.spawn().map_err(|e| format!("cannot start {}: {e}", job.claude))?;
    rec.claude_pid = Some(child.id());
    let _ = runs::save(rec);

    let (code, timed_out) = wait(&mut child, Duration::from_secs(u64::from(job.timeout_minutes) * 60));
    rec.exit_code = Some(code);
    let output: serde_json::Value = std::fs::read_to_string(&out_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::Value::Null);
    rec.cost_usd = output.get("total_cost_usd").and_then(serde_json::Value::as_f64);
    rec.num_turns = output.get("num_turns").and_then(serde_json::Value::as_u64).map(|n| n as u32);
    rec.session_id = output.get("session_id").and_then(serde_json::Value::as_str).map(str::to_string);
    rec.result = output.get("result").and_then(serde_json::Value::as_str).map(|r| r.chars().take(20_000).collect());
    let is_error = output.get("is_error").and_then(serde_json::Value::as_bool).unwrap_or(false) || code != 0;
    rec.status = if timed_out {
        RunStatus::Timeout
    } else if is_error {
        RunStatus::Failed
    } else {
        RunStatus::Ok
    };
    if is_error && !timed_out && login_refused(&output) {
        return Ok(Outcome::LoginExpired);
    }
    apply_verdict(rec);
    if rec.status != RunStatus::Ok && rec.error.is_none() {
        let stderr = std::fs::read_to_string(&err_path).unwrap_or_default();
        let stderr_tail: String = stderr.chars().rev().take(2000).collect::<Vec<_>>().into_iter().rev().collect();
        rec.error = Some(match (stderr_tail.trim(), rec.result.as_deref()) {
            (s, _) if !s.is_empty() => s.to_string(),
            (_, Some(r)) if !r.is_empty() => r.to_string(),
            _ if timed_out => format!("stopped after {} minutes", job.timeout_minutes),
            _ => format!("claude exited with code {code}"),
        });
    }
    Ok(Outcome::Finished)
}

/// Whether `claude -p` was refused for its login: the API said 401, or Claude Code asked for /login.
fn login_refused(output: &serde_json::Value) -> bool {
    output.get("api_error_status").and_then(serde_json::Value::as_u64) == Some(401)
        || output.get("result").and_then(serde_json::Value::as_str).is_some_and(is_login_error)
}

fn wait(child: &mut Child, limit: Duration) -> (i32, bool) {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return (status.code().unwrap_or(-1), false),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(500)),
            _ => break,
        }
    }
    let group = child.id() as libc::pid_t;
    unsafe { libc::killpg(group, libc::SIGTERM) };
    let grace = Instant::now() + Duration::from_secs(20);
    while Instant::now() < grace {
        if let Ok(Some(status)) = child.try_wait() {
            return (status.code().unwrap_or(-1), true);
        }
        thread::sleep(Duration::from_millis(500));
    }
    unsafe { libc::killpg(group, libc::SIGKILL) };
    let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
    (code, true)
}

/// Stays on screen until closed. A notification would not: macOS drops `display notification` from osascript unless
/// Script Editor was once allowed to notify, and a banner shown at login is easy to miss.
fn alert(title: &str, message: &str) {
    const SCRIPT: &str = r#"on run argv
    set {heading, body, appPath} to argv
    activate
    if appPath is "" then
        display alert heading message body as critical
        return
    end if
    set choice to button returned of (display alert heading message body as critical ¬
        buttons {"Close", "Open Claude Jobs"} default button "Open Claude Jobs")
    if choice is "Open Claude Jobs" then do shell script "/usr/bin/open " & quoted form of appPath
end run"#;
    let exe = std::env::current_exe().unwrap_or_default();
    let app = app_bundle(&exe).map(|p| p.display().to_string()).unwrap_or_default();
    let message: String = message.chars().take(500).collect();
    // Not waited on, and in its own process group so it stays up after the run exits and launchd clears the job.
    let _ = Command::new("/usr/bin/osascript")
        .args(["-e", SCRIPT, title, &message, &app])
        .stdin(Stdio::null())
        .process_group(0)
        .spawn();
}

/// The `.app` the binary is in; none for a dev build in `target/debug`.
fn app_bundle(exe: &Path) -> Option<&Path> {
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(result: &str) -> RunRecord {
        let text = format!(
            r#"{{"id": "20261027T083651Z", "job": "j", "trigger": "catch-up", "status": "ok",
                "started_at": "2026-10-27T08:36:51Z", "result": {}}}"#,
            serde_json::to_string(result).expect("string")
        );
        serde_json::from_str(&text).expect("parses")
    }

    #[test]
    fn a_run_claude_reports_as_failed_is_failed() {
        let mut rec = finished("Nothing was updated this run.\n\nRun gcloud auth login.\n\n`RUN STATUS: FAILED`\n");
        apply_verdict(&mut rec);
        assert_eq!(rec.status, RunStatus::Failed);
        assert_eq!(rec.error.as_deref(), Some("Nothing was updated this run."));
        assert_eq!(rec.result.as_deref(), Some("Nothing was updated this run.\n\nRun gcloud auth login."));
    }

    #[test]
    fn a_run_reported_ok_or_without_a_status_stays_ok() {
        let mut rec = finished("Dashboard updated.\nRUN STATUS: OK");
        apply_verdict(&mut rec);
        assert_eq!((rec.status, rec.result.as_deref()), (RunStatus::Ok, Some("Dashboard updated.")));
        let mut rec = finished("Dashboard updated.");
        apply_verdict(&mut rec);
        assert_eq!((rec.status, rec.result.as_deref()), (RunStatus::Ok, Some("Dashboard updated.")));
    }

    #[test]
    fn a_headless_run_refused_for_its_login_is_told_apart() {
        let refused = serde_json::json!({"is_error": true, "api_error_status": 401,
            "result": "Failed to authenticate. API Error: 401 Invalid bearer token"});
        assert!(login_refused(&refused));
        assert!(login_refused(&serde_json::json!({"is_error": true, "result": "Login expired · Please run /login"})));
        assert!(!login_refused(
            &serde_json::json!({"is_error": true, "api_error_status": 529, "result": "Overloaded"})
        ));
    }

    #[test]
    fn the_alert_opens_the_app_the_run_came_from() {
        let exe = Path::new("/Users/x/Applications/Claude Jobs.app/Contents/MacOS/claude-jobs");
        assert_eq!(app_bundle(exe), Some(Path::new("/Users/x/Applications/Claude Jobs.app")));
        assert_eq!(app_bundle(Path::new("/Users/x/claude-jobs/src-tauri/target/debug/claude-jobs")), None);
    }
}
