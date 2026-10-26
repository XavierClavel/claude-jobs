//! An interactive run: a real Claude Code session in tmux, for tools a headless `claude -p` does not get (the
//! Artifact tools). A Stop hook tells the runner when Claude has finished; the runner then closes the session and
//! reads the result and cost from the session transcript.

use crate::jobs::Job;
use crate::runs::{self, RunRecord, RunStatus};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

/// A tmux server of its own, so job sessions never mix with the user's.
pub const SOCKET: &str = "claude-jobs";

pub fn session_name(job_id: &str) -> String {
    format!("cj-{job_id}")
}

pub fn tmux_bin(path: &str) -> Option<PathBuf> {
    path.split(':')
        .map(|dir| Path::new(dir).join("tmux"))
        .chain(["/opt/homebrew/bin/tmux", "/usr/local/bin/tmux"].map(PathBuf::from))
        .find(|p| p.is_file())
}

fn tmux(bin: &Path, args: &[&str]) -> Option<Output> {
    Command::new(bin).arg("-L").arg(SOCKET).args(args).output().ok()
}

pub fn has_session(bin: &Path, session: &str) -> bool {
    tmux(bin, &["has-session", "-t", session]).is_some_and(|o| o.status.success())
}

/// A single-quoted shell word.
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

pub fn execute(job: &Job, rec: &mut RunRecord) -> Result<(), String> {
    let bin = tmux_bin(&job.path).ok_or("tmux is not installed (brew install tmux)")?;
    let dir = runs::dir(&job.id);
    let marker = dir.join(format!("{}.done", rec.id));
    let settings = dir.join(format!("{}.settings.json", rec.id));
    let out_path = dir.join(format!("{}.out", rec.id));
    // The Stop hook gets the session id and transcript path on stdin; keeping them is the "finished" signal.
    let hook = serde_json::json!({
        "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": format!("cat > {}", shell_quote(&marker.display().to_string())) } ] } ] }
    });
    fs::write(&settings, hook.to_string()).map_err(|e| e.to_string())?;

    let session = session_name(&job.id);
    // A session left over from a run that was cut short would block this one.
    let _ = tmux(&bin, &["kill-session", "-t", &session]);
    let home = crate::paths::home().display().to_string();
    let user = std::env::var("USER").unwrap_or_default();
    let env = [
        format!("PATH={}", job.path),
        format!("HOME={home}"),
        format!("USER={user}"),
        "SHELL=/bin/zsh".to_string(),
        "TERM=xterm-256color".to_string(),
        "LANG=en_US.UTF-8".to_string(),
    ];
    let mut args: Vec<String> =
        ["new-session", "-d", "-s", &session, "-x", "220", "-y", "50", "-c", &job.cwd, "--", "env", "-i"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    args.extend(env);
    args.extend([job.claude.clone(), "--settings".into(), settings.display().to_string()]);
    if !job.model.is_empty() {
        args.extend(["--model".into(), job.model.clone()]);
    }
    if !job.allowed_tools.is_empty() {
        args.push("--allowedTools".into());
        args.extend(job.allowed_tools.iter().cloned());
    }
    args.extend(["--".into(), job.prompt.clone()]);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let started = tmux(&bin, &arg_refs).ok_or("cannot run tmux")?;
    if !started.status.success() {
        return Err(format!("tmux could not start the session: {}", String::from_utf8_lossy(&started.stderr).trim()));
    }
    rec.tmux_session = Some(session.clone());
    let _ = runs::save(rec);

    let deadline = Instant::now() + Duration::from_secs(u64::from(job.timeout_minutes) * 60);
    let finished = loop {
        if let Some(stop) = read_marker(&marker) {
            break Some(stop);
        }
        if !has_session(&bin, &session) {
            break None;
        }
        if Instant::now() >= deadline {
            break None;
        }
        thread::sleep(Duration::from_secs(1));
    };
    let timed_out = finished.is_none() && has_session(&bin, &session);
    capture(&bin, &session, &out_path);
    close(&bin, &session);

    match finished {
        Some(stop) => {
            rec.session_id = stop.get("session_id").and_then(|v| v.as_str()).map(str::to_string);
            if let Some(transcript) = stop.get("transcript_path").and_then(|v| v.as_str()) {
                let summary = read_transcript(Path::new(transcript));
                rec.result = summary.result;
                rec.num_turns = Some(summary.turns);
                rec.cost_usd = summary.cost_usd;
            }
            rec.status = RunStatus::Ok;
        }
        None if timed_out => {
            rec.status = RunStatus::Timeout;
            rec.error = Some(format!(
                "stopped after {} minutes: Claude never finished, often a permission prompt for a tool that is not \
                 in the allowed list. The last screen is in the run's output.",
                job.timeout_minutes
            ));
        }
        None => {
            rec.status = RunStatus::Failed;
            rec.error =
                Some("the Claude session closed before finishing. Its last screen is in the run's output.".into());
        }
    }
    Ok(())
}

fn read_marker(path: &Path) -> Option<serde_json::Value> {
    // The hook may still be writing: only a complete JSON document counts.
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// What the pane shows, scrollback included: the record of the run when something went wrong.
fn capture(bin: &Path, session: &str, out: &Path) {
    if let Some(o) = tmux(bin, &["capture-pane", "-p", "-J", "-S", "-", "-t", session]) {
        let _ = fs::write(out, o.stdout);
    }
}

/// `/exit` lets Claude write its final cost to the transcript; the session is killed if it does not go.
fn close(bin: &Path, session: &str) {
    if !has_session(bin, session) {
        return;
    }
    let _ = tmux(bin, &["send-keys", "-t", session, "Escape"]);
    let _ = tmux(bin, &["send-keys", "-t", session, "/exit", "Enter"]);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && has_session(bin, session) {
        thread::sleep(Duration::from_millis(500));
    }
    let _ = tmux(bin, &["kill-session", "-t", session]);
}

#[derive(Debug, PartialEq)]
pub struct TranscriptSummary {
    pub result: Option<String>,
    pub turns: u32,
    pub cost_usd: Option<f64>,
}

/// The last thing Claude said, how many replies it took, and the session cost Claude Code recorded.
pub fn read_transcript(path: &Path) -> TranscriptSummary {
    let mut summary = TranscriptSummary { result: None, turns: 0, cost_usd: None };
    // The cost is written as the session closes: give it a moment to land.
    for _ in 0..10 {
        let Ok(text) = fs::read_to_string(path) else { return summary };
        summary = TranscriptSummary { result: None, turns: 0, cost_usd: None };
        for entry in text.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()) {
            match entry.get("type").and_then(|t| t.as_str()) {
                Some("assistant") => {
                    summary.turns += 1;
                    let texts: Vec<&str> = entry
                        .pointer("/message/content")
                        .and_then(|c| c.as_array())
                        .into_iter()
                        .flatten()
                        .filter(|c| c.get("type").and_then(|t| t.as_str()) == Some("text"))
                        .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                        .collect();
                    if !texts.is_empty() {
                        summary.result = Some(texts.join("\n").chars().take(20_000).collect());
                    }
                }
                Some("cost-state") => {
                    summary.cost_usd = entry.get("totalCostUSD").and_then(|v| v.as_f64());
                }
                _ => {}
            }
        }
        if summary.cost_usd.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(500));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_survives_quotes_and_spaces() {
        assert_eq!(shell_quote("/a b/it's.done"), r"'/a b/it'\''s.done'");
    }

    #[test]
    fn transcript_gives_last_text_turns_and_cost() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("t.jsonl");
        let lines = [
            r#"{"type":"user","message":{"content":"go"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done: 3 orders."}]}}"#,
            r#"{"type":"cost-state","totalCostUSD":0.42}"#,
        ];
        fs::write(&path, lines.join("\n")).expect("write");
        assert_eq!(
            read_transcript(&path),
            TranscriptSummary { result: Some("Done: 3 orders.".into()), turns: 2, cost_usd: Some(0.42) }
        );
    }
}
