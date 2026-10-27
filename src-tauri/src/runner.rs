//! One headless run of a job: `claude -p <prompt> --output-format json`, recorded in the job's history.

use crate::jobs::{self, Job};
use crate::runs::{self, RunRecord, RunStatus, Trigger};
use crate::{paths, schedule};
use chrono::{DateTime, Local, SecondsFormat, Utc};
use std::fs::File;
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const NETWORK_WAIT: Duration = Duration::from_secs(300);

/// A session that finishes is not a run that worked: Claude says which it was on the last line of its answer.
pub const STATUS_INSTRUCTION: &str = "This is an unattended scheduled run that nobody watches. End your final \
message with a line of its own: `RUN STATUS: FAILED` when the task could not be done (for example a data source, \
credential or tool failed and you stopped), otherwise `RUN STATUS: OK`.";
const STATUS_PREFIX: &str = "RUN STATUS:";

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
    if runs::is_running(job_id) {
        eprintln!("{job_id} is already running");
        return 3;
    }
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
        return 1;
    }

    let outcome = if job.interactive { crate::interactive::execute(&job, &mut rec) } else { execute(&job, &mut rec) };
    let ended = Utc::now();
    rec.ended_at = Some(ended.to_rfc3339_opts(SecondsFormat::Secs, true));
    rec.duration_s = Some((ended - started).num_seconds().max(0) as u32);
    match outcome {
        Ok(()) => {}
        Err(e) => {
            rec.status = RunStatus::Failed;
            rec.error = Some(e);
        }
    }
    if rec.status != RunStatus::Ok {
        let last_line = rec.error.as_deref().and_then(|e| e.lines().last()).unwrap_or("The run did not finish.");
        notify(&format!("Claude job failed: {}", job.name), last_line);
    }
    let _ = runs::save(&rec);
    runs::prune(job_id);
    if rec.status == RunStatus::Ok {
        0
    } else {
        1
    }
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

fn execute(job: &Job, rec: &mut RunRecord) -> Result<(), String> {
    let dir = runs::dir(&job.id);
    let out_path = dir.join(format!("{}.out", rec.id));
    let err_path = dir.join(format!("{}.err", rec.id));
    let mut cmd = Command::new(&job.claude);
    cmd.args(["-p", &job.prompt, "--output-format", "json", "--append-system-prompt", STATUS_INSTRUCTION]);
    if !job.model.is_empty() {
        cmd.args(["--model", &job.model]);
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
    Ok(())
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

/// An AppleScript string literal: quotes and backslashes escaped, control characters dropped.
fn applescript_string(text: &str) -> String {
    let clean: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).take(200).collect();
    format!("\"{}\"", clean.replace('\\', "\\\\").replace('"', "\\\""))
}

fn notify(title: &str, message: &str) {
    let script =
        format!("display notification {} with title {}", applescript_string(message), applescript_string(title));
    let _ = Command::new("osascript").args(["-e", &script]).output();
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
    fn notification_text_is_escaped() {
        assert_eq!(applescript_string("a \"b\" \\ c\nd"), "\"a \\\"b\\\" \\\\ c d\"");
    }
}
