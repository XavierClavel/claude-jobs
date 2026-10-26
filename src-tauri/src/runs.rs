//! Run history: one `<run>.json` per run in `~/.claude-jobs/runs/<job>/`, with Claude's output and errors beside it.

use crate::{jobs, paths};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use ts_rs::TS;

pub const KEEP_RUNS: usize = 100;
const DETAIL_TAIL: usize = 200_000;

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum RunStatus {
    Running,
    Ok,
    Failed,
    Timeout,
    Interrupted,
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Trigger {
    Schedule,
    Manual,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug)]
#[ts(export)]
pub struct RunRecord {
    pub id: String,
    pub job: String,
    pub trigger: Trigger,
    pub status: RunStatus,
    pub started_at: String,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub duration_s: Option<u32>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub num_turns: Option<u32>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub claude_pid: Option<u32>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub interactive: bool,
    /// The tmux session of an interactive run, which the app can open to watch it.
    #[serde(default)]
    pub tmux_session: Option<String>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[ts(export)]
pub struct RunDetail {
    #[serde(flatten)]
    pub run: RunRecord,
    /// Claude's raw output, the JSON of `--output-format json`.
    pub out: String,
    /// What Claude wrote to stderr.
    pub err: String,
}

pub fn dir(job_id: &str) -> PathBuf {
    paths::runs_dir().join(job_id)
}

pub fn valid_run_id(id: &str) -> bool {
    // 20261026T134651Z
    id.len() == 16
        && id.char_indices().all(|(i, c)| match i {
            8 => c == 'T',
            15 => c == 'Z',
            _ => c.is_ascii_digit(),
        })
}

pub fn pid_alive(pid: u32) -> bool {
    let res = unsafe { libc::kill(pid as libc::pid_t, 0) };
    res == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// A run recorded as running whose process is gone was cut short: the Mac shut down, or the runner was killed.
fn with_liveness(mut rec: RunRecord) -> RunRecord {
    if rec.status == RunStatus::Running && !rec.pid.is_some_and(pid_alive) {
        rec.status = RunStatus::Interrupted;
    }
    rec
}

fn run_files(job_id: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir(job_id)) else { return vec![] };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json")
                && p.file_stem().and_then(|s| s.to_str()).is_some_and(valid_run_id)
        })
        .collect();
    // Run ids are UTC timestamps, so the newest sorts last.
    files.sort();
    files.reverse();
    files
}

pub fn list(job_id: &str, limit: usize) -> Vec<RunRecord> {
    run_files(job_id)
        .into_iter()
        .take(limit)
        .filter_map(|p| fs::read_to_string(p).ok())
        .filter_map(|text| serde_json::from_str::<RunRecord>(&text).ok())
        .map(with_liveness)
        .collect()
}

pub fn is_running(job_id: &str) -> bool {
    list(job_id, 5).iter().any(|r| r.status == RunStatus::Running)
}

pub fn save(rec: &RunRecord) -> Result<(), String> {
    let d = dir(&rec.job);
    fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let path = d.join(format!("{}.json", rec.id));
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_string_pretty(rec).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn prune(job_id: &str) {
    for old in run_files(job_id).into_iter().skip(KEEP_RUNS) {
        for ext in ["json", "out", "err", "done", "settings.json"] {
            let _ = fs::remove_file(old.with_extension(ext));
        }
    }
}

fn tail(text: String) -> String {
    if text.len() <= DETAIL_TAIL {
        return text;
    }
    let mut start = text.len() - DETAIL_TAIL;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

pub fn detail(job_id: &str, run_id: &str) -> Result<RunDetail, String> {
    if !jobs::valid_id(job_id) || !valid_run_id(run_id) {
        return Err("No such run.".to_string());
    }
    let d = dir(job_id);
    let text = fs::read_to_string(d.join(format!("{run_id}.json"))).map_err(|_| "No such run.".to_string())?;
    let run = with_liveness(serde_json::from_str(&text).map_err(|e| format!("The run record does not parse: {e}"))?);
    let read = |ext: &str| tail(fs::read_to_string(d.join(format!("{run_id}.{ext}"))).unwrap_or_default());
    Ok(RunDetail { run, out: read("out"), err: read("err") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_ids_are_utc_timestamps() {
        assert!(valid_run_id("20261026T134651Z"));
        assert!(!valid_run_id("20261026T134651"));
        assert!(!valid_run_id("../../etc/passwd"));
        assert!(!valid_run_id("2026102XT134651Z"));
    }

    #[test]
    fn records_from_the_python_version_parse() {
        let text = r#"{"id": "20261026T134651Z", "job": "photobook-monitor", "trigger": "manual", "status": "ok",
            "started_at": "2026-10-26T13:46:51Z", "pid": 1, "prompt": "/photobook-monitor", "claude_pid": 2,
            "ended_at": "2026-10-26T13:47:54Z", "duration_s": 63, "exit_code": 0, "cost_usd": 0.553077,
            "num_turns": 9, "session_id": "3a57", "result": "done"}"#;
        let rec: RunRecord = serde_json::from_str(text).expect("parses");
        assert_eq!(rec.status, RunStatus::Ok);
        assert_eq!(rec.trigger, Trigger::Manual);
        assert_eq!(rec.duration_s, Some(63));
    }

    #[test]
    fn a_running_record_without_a_live_process_is_interrupted() {
        let text = r#"{"id": "20261026T134651Z", "job": "j", "trigger": "schedule", "status": "running",
            "started_at": "2026-10-26T13:46:51Z", "pid": 999999999}"#;
        let rec = with_liveness(serde_json::from_str(text).expect("parses"));
        assert_eq!(rec.status, RunStatus::Interrupted);
    }
}
