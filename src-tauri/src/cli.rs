//! What other apps call on the command line, such as Dooock's Claude Jobs widget. Each command does what the window's
//! button of the same name does, so the two never disagree about a job.

use crate::commands::{self, ApiError, JobView};
use crate::runs::{self, RunRecord};
use crate::schedule;
use chrono::{Local, SecondsFormat};
use serde::Serialize;

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
    match serde_json::to_string(&Status { version: STATUS_VERSION, jobs }) {
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
        };
        let previous_run = Some("2026-10-29T09:00:00+01:00".into());
        let status =
            Status { version: STATUS_VERSION, jobs: vec![JobStatus { view, previous_run, recent: vec![run] }] };
        let value = serde_json::to_value(&status).expect("serializes");
        assert_eq!(value["version"], 1);
        let job = &value["jobs"][0];
        for key in ["id", "name", "prompt", "hour", "minute", "weekdays", "month_days", "enabled", "interactive"] {
            assert!(job.get(key).is_some(), "{key} is missing");
        }
        assert_eq!(job["next_run"], "2026-10-30T09:00:00+01:00");
        assert_eq!(job["previous_run"], "2026-10-29T09:00:00+01:00");
        let run = &job["recent"][0];
        for key in ["id", "status", "trigger", "started_at", "duration_s", "cost_usd", "result"] {
            assert!(run.get(key).is_some(), "recent run {key} is missing");
        }
    }
}
