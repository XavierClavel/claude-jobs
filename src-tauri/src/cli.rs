//! What other apps call on the command line, such as Dooock's Claude Jobs widget. Each command does what the window's
//! button of the same name does, so the two never disagree about a job.

use crate::commands::{self, ApiError, JobView};
use crate::jobs::{self, Job};
use crate::runs::{self, RunRecord};
use crate::schedule;
use chrono::{Local, SecondsFormat};
use serde::Serialize;
use serde_json::Value;
use std::io::Read;

/// Runs listed per job in `status`: enough for a row of recent results.
const RECENT_RUNS: usize = 10;

/// Changes when a field of `status` goes away or changes meaning. Adding a field does not change it.
const STATUS_VERSION: u32 = 1;

#[derive(Serialize)]
struct Status {
    version: u32,
    jobs: Vec<JobStatus>,
}

#[derive(Serialize)]
struct JobStatus {
    #[serde(flatten)]
    view: JobView,
    /// The last scheduled time at or before now, paused or not. With `next_run`, it tells whether the job is due
    /// today once today's time has passed.
    previous_run: Option<String>,
    /// The latest runs, newest first.
    recent: Vec<RunRecord>,
}

/// `claude-jobs status`: every job with its previous and next scheduled times and latest runs, as JSON on stdout.
pub fn status() -> i32 {
    let views = match commands::list_jobs() {
        Ok(views) => views,
        Err(e) => return fail(e),
    };
    let now = Local::now();
    let jobs = views
        .into_iter()
        .map(|view| JobStatus {
            previous_run: schedule::previous_run(&view.job, now).map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, false)),
            recent: runs::list(&view.job.id, RECENT_RUNS),
            view,
        })
        .collect();
    print_json(&Status { version: STATUS_VERSION, jobs })
}

/// `claude-jobs save`: creates or changes a job from JSON on stdin, like **Save** in the editor, and prints it the way
/// `status` lists it. A new job needs `id`, `name`, `prompt`, `hour` and `minute`, and gets the editor's defaults for
/// the rest. For an existing id, the fields given replace the job's and the others stay as they are.
pub fn save() -> i32 {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        return fail(format!("cannot read the job from stdin: {e}").into());
    }
    let result = jobs::load()
        .map_err(ApiError::from)
        .and_then(|all| job_from_input(&input, &all).map_err(ApiError::from))
        .and_then(|(job, editing)| commands::save_job(job, editing));
    match result {
        Ok(view) => print_json(&view),
        Err(e) => fail(e),
    }
}

/// The job `input` describes, over the job of the same id if there is one, with that id when there is.
fn job_from_input(input: &str, existing: &[Job]) -> Result<(Job, Option<String>), String> {
    let given: Value = serde_json::from_str(input).map_err(|e| format!("The job is not JSON: {e}"))?;
    let Value::Object(fields) = given else {
        return Err("The job must be a JSON object.".to_string());
    };
    let id = fields.get("id").and_then(Value::as_str).ok_or("The job needs an \"id\".")?;
    let current = existing.iter().find(|j| j.id == id);
    let mut merged = match current.map(serde_json::to_value) {
        Some(Ok(Value::Object(job))) => job,
        Some(_) => return Err(format!("Cannot read the job {id} to change it.")),
        None => serde_json::Map::new(),
    };
    merged.extend(fields.clone());
    let job: Job = serde_json::from_value(Value::Object(merged)).map_err(|e| format!("The job does not fit: {e}"))?;
    // serde skips fields it does not know, so a misspelt one would be lost without a word.
    let known = serde_json::to_value(&job).map_err(|e| e.to_string())?;
    let unknown: Vec<&str> = fields.keys().map(String::as_str).filter(|k| known.get(k).is_none()).collect();
    if !unknown.is_empty() {
        return Err(format!("Unknown fields: {}.", unknown.join(", ")));
    }
    Ok((job, current.map(|j| j.id.clone())))
}

fn print_json<T: Serialize>(value: &T) -> i32 {
    match serde_json::to_string(value) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => fail(e.to_string().into()),
    }
}

/// `claude-jobs start <job>`: starts a run now, like **Run now**.
pub fn start(job_id: &str) -> i32 {
    reply(commands::run_now(job_id.to_string()))
}

/// `claude-jobs watch <job>`: opens Terminal on a live interactive run, like **Watch**.
pub fn watch(job_id: &str) -> i32 {
    reply(commands::watch_run(job_id.to_string()))
}

fn reply(result: Result<(), ApiError>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(e) => fail(e),
    }
}

fn fail(e: ApiError) -> i32 {
    eprintln!("{}", e.errors.join("\n"));
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditions::{Condition, ConditionStatus, CLAUDE_EXPIRED};
    use crate::jobs::Job;

    #[test]
    fn status_keeps_the_fields_other_apps_read() {
        let job: Job = serde_json::from_str(r#"{"id": "j", "name": "J", "prompt": "/j", "hour": 9, "minute": 0}"#)
            .expect("parses");
        let run: RunRecord = serde_json::from_str(
            r#"{"id": "20261029T081253Z", "job": "j", "trigger": "manual", "status": "ok",
                "started_at": "2026-10-29T08:12:53Z", "duration_s": 173, "cost_usd": 0.85, "result": "done"}"#,
        )
        .expect("parses");
        let view = JobView {
            job,
            label: "com.claude-jobs.j".into(),
            loaded: true,
            next_run: Some("2026-10-30T09:00:00+01:00".into()),
            last_run: Some(run.clone()),
            waiting: None,
            unmet_conditions: vec![ConditionStatus {
                condition: Condition::Claude,
                problem: Some(CLAUDE_EXPIRED.into()),
            }],
        };
        let previous_run = Some("2026-10-29T09:00:00+01:00".into());
        let status =
            Status { version: STATUS_VERSION, jobs: vec![JobStatus { view, previous_run, recent: vec![run] }] };
        let value = serde_json::to_value(&status).expect("serializes");
        assert_eq!(value["version"], 1);
        let job = &value["jobs"][0];
        for key in
            ["id", "name", "prompt", "hour", "minute", "weekdays", "month_days", "enabled", "interactive", "waiting"]
        {
            assert!(job.get(key).is_some(), "{key} is missing");
        }
        assert_eq!(job["conditions"], serde_json::json!([]));
        assert_eq!(job["unmet_conditions"][0]["condition"], "claude");
        assert_eq!(job["unmet_conditions"][0]["problem"], CLAUDE_EXPIRED);
        assert_eq!(job["next_run"], "2026-10-30T09:00:00+01:00");
        assert_eq!(job["previous_run"], "2026-10-29T09:00:00+01:00");
        let run = &job["recent"][0];
        for key in ["id", "status", "trigger", "started_at", "duration_s", "cost_usd", "result"] {
            assert!(run.get(key).is_some(), "recent run {key} is missing");
        }
    }

    const NEW_JOB: &str = r#"{"id": "j", "name": "J", "prompt": "/j", "hour": 9, "minute": 0}"#;

    #[test]
    fn save_makes_a_new_job_with_the_defaults() {
        let (job, editing) = job_from_input(NEW_JOB, &[]).expect("valid");
        assert_eq!(editing, None);
        assert_eq!((job.id.as_str(), job.hour, job.timeout_minutes, job.enabled), ("j", 9, 30, true));
        assert!(!job.chrome && job.cwd.is_empty());
    }

    #[test]
    fn save_changes_only_the_fields_given() {
        let (mut current, _) = job_from_input(NEW_JOB, &[]).expect("valid");
        current.allowed_tools = vec!["Read".into()];
        let (job, editing) =
            job_from_input(r#"{"id": "j", "enabled": false, "chrome": true}"#, &[current]).expect("valid");
        assert_eq!(editing.as_deref(), Some("j"));
        assert!(!job.enabled && job.chrome);
        assert_eq!((job.prompt.as_str(), job.allowed_tools.clone()), ("/j", vec!["Read".to_string()]));
    }

    #[test]
    fn save_refuses_what_it_cannot_read() {
        let error = |input: &str| job_from_input(input, &[]).unwrap_err();
        assert!(error("not json").starts_with("The job is not JSON"));
        assert_eq!(error("[]"), "The job must be a JSON object.");
        assert_eq!(error(r#"{"name": "J"}"#), "The job needs an \"id\".");
        assert!(error(r#"{"id": "j", "name": "J"}"#).contains("prompt"));
        let misspelt = NEW_JOB.replace("\"minute\": 0", "\"minute\": 0, \"promt\": \"/x\"");
        assert_eq!(error(&misspelt), "Unknown fields: promt.");
    }
}
