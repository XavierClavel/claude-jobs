//! What a job does once a run ends: the actions it names in `on_success`, for a run that went well, `on_failure`, for
//! one that failed or timed out, and `on_complete`, for either. An action is defined once, in `~/.claude-jobs/actions.json`, and any job can use
//! it. The one kind so far starts a Slack workflow, one that begins "From a webhook" in Workflow Builder, with the
//! run's result and the user or channel to send it to as its variables.
//!
//! Such a workflow reads flat JSON whose keys are the variables it declares, and fails when one of them is missing:
//! every message sends the same keys, see `Message::payload`.

use crate::jobs::{self, Job};
use crate::paths;
use crate::runs::{RunRecord, RunStatus, Trigger};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::thread;
use std::time::Duration;
use ts_rs::TS;

const SEND_TIMEOUT: Duration = Duration::from_secs(15);
/// What a message keeps of Claude's result: Slack folds a long one anyway, and the whole of it is in the app.
const MESSAGE_LIMIT: usize = 3000;
const WEBHOOK_PREFIXES: [&str; 2] = ["https://hooks.slack.com/", "https://hooks.slack-gov.com/"];

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub target: Target,
}

/// Where an action sends the run's result.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Target {
    /// A Slack workflow started by its webhook, which sends the message to the user or channel it is given.
    Slack {
        webhook_url: String,
        /// Whether `recipient` is a user or a channel. The workflow gets it in the variable of that name, `user` or
        /// `channel`.
        to: SlackTo,
        /// The user's or the channel's Slack ID, such as `U012AB3CD` or `C012AB3CD`.
        recipient: String,
    },
}

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SlackTo {
    Channel,
    /// A user's direct messages.
    User,
}

impl SlackTo {
    /// The workflow's variable for the recipient, a Channel ID or a User ID: the same as the value's JSON.
    pub fn key(self) -> &'static str {
        match self {
            SlackTo::Channel => "channel",
            SlackTo::User => "user",
        }
    }
}

/// What an action did once a run ended, kept in the run's record.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ActionResult {
    pub action: String,
    /// Its name then: it may have been renamed or deleted since.
    pub name: String,
    /// Why it did not work; none when it did.
    pub error: Option<String>,
}

pub fn load() -> Result<Vec<Action>, String> {
    match fs::read_to_string(paths::actions_file()) {
        // As for jobs.json: a file that does not parse is never "no actions", or saving would lose them.
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("actions.json does not parse: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(format!("cannot read actions.json: {e}")),
    }
}

pub fn save(actions: &[Action]) -> Result<(), String> {
    fs::create_dir_all(paths::base()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(actions).map_err(|e| e.to_string())?;
    write_private(&paths::actions_file(), &text)
}

/// Anyone with a webhook can post with it: the file is readable by its owner alone.
fn write_private(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    // The mode only applies to a new file, so a leftover one goes first.
    let _ = fs::remove_file(&tmp);
    let mut file =
        fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// `input` tidied up, or what is wrong with it. `editing` is the id of the action it changes, if any.
pub fn validate(input: Action, existing: &[Action], editing: Option<&str>) -> Result<Action, Vec<String>> {
    let mut errors = Vec::new();
    let target = match input.target {
        Target::Slack { webhook_url, to, recipient } => {
            Target::Slack { webhook_url: webhook_url.trim().to_string(), to, recipient: recipient.trim().to_string() }
        }
    };
    let action = Action { id: input.id.trim().to_string(), name: input.name.trim().to_string(), target };
    if !jobs::valid_id(&action.id) {
        errors.push("Id: lowercase letters, digits and dashes, 40 characters at most.".to_string());
    }
    match editing {
        Some(current) if current != action.id => errors.push("An action's id cannot change.".to_string()),
        None if existing.iter().any(|a| a.id == action.id) => {
            errors.push(format!("An action called {} already exists.", action.id))
        }
        _ => {}
    }
    if action.name.is_empty() {
        errors.push("Name is required.".to_string());
    }
    match &action.target {
        Target::Slack { webhook_url, to, recipient } => {
            let valid_url = WEBHOOK_PREFIXES.iter().any(|prefix| {
                webhook_url
                    .strip_prefix(prefix)
                    .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
            });
            if !valid_url {
                errors.push(
                    "The webhook URL starts with https://hooks.slack.com/triggers/: copy it from the workflow's \
                     \"From a webhook\" step in Workflow Builder."
                        .to_string(),
                );
            }
            if !valid_slack_id(recipient, *to) {
                errors.push(match to {
                    SlackTo::User => "A user ID looks like U012AB3CD, not a name: in Slack, open the user's \
                                      profile, click ⋮, then Copy member ID."
                        .to_string(),
                    SlackTo::Channel => "A channel ID looks like C012AB3CD, not a name: in Slack, open the \
                                         channel's details; it is at the bottom of About."
                        .to_string(),
                });
            }
        }
    }
    if errors.is_empty() {
        Ok(action)
    } else {
        Err(errors)
    }
}

/// A Slack ID as Slack writes it: `U012AB3CD` for a user (`W…` in Enterprise Grid), `C012AB3CD` for a channel (`G…`
/// for an older private one).
fn valid_slack_id(id: &str, to: SlackTo) -> bool {
    let first = match to {
        SlackTo::User => ['U', 'W'],
        SlackTo::Channel => ['C', 'G'],
    };
    id.starts_with(first)
        && (9..=15).contains(&id.len())
        && id.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

pub fn is_known(id: &str, actions: &[Action]) -> bool {
    actions.iter().any(|a| a.id == id)
}

/// The actions `job` takes for a run that ended as `status`: those for its outcome, then those for any outcome, each
/// once. None for a run still going or cut short, which has no runner left to take them.
pub fn for_outcome(job: &Job, status: RunStatus) -> Vec<String> {
    let outcome = match status {
        RunStatus::Ok => &job.on_success,
        RunStatus::Failed | RunStatus::Timeout => &job.on_failure,
        RunStatus::Running | RunStatus::Interrupted => return vec![],
    };
    let mut ids: Vec<String> = Vec::new();
    for id in outcome.iter().chain(&job.on_complete) {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids
}

/// Takes the actions `job` names for how `rec` ended, and says what each did. One deleted since is left out.
pub fn after_run(job: &Job, rec: &RunRecord) -> Vec<ActionResult> {
    let ids = for_outcome(job, rec.status);
    if ids.is_empty() {
        return vec![];
    }
    let all = match load() {
        Ok(all) => all,
        Err(e) => {
            return ids
                .iter()
                .map(|id| ActionResult { action: id.clone(), name: id.clone(), error: Some(e.clone()) })
                .collect()
        }
    };
    let message = Message::for_run(job, rec);
    ids.iter()
        .filter_map(|id| all.iter().find(|a| &a.id == id))
        .map(|action| ActionResult {
            action: action.id.clone(),
            name: action.name.clone(),
            error: perform(action, &message).err(),
        })
        .collect()
}

/// Sends a message saying what the action is for, with every variable a run sends, so the workflow can be tried
/// before a run needs it.
pub fn test(action: &Action) -> Result<(), String> {
    let message = Message {
        job: "Claude Jobs test".to_string(),
        status: "Test".to_string(),
        summary: format!("👋 Test from Claude Jobs: {}", action.name),
        result: "When a job that uses this action ends, how the run went and Claude's result come here.".to_string(),
    };
    perform(action, &message)
}

fn perform(action: &Action, message: &Message) -> Result<(), String> {
    match &action.target {
        Target::Slack { webhook_url, .. } => post(webhook_url, message.payload(&action.target).to_string()),
    }
}

/// A run's message, each part a variable of the workflow.
#[derive(Debug, PartialEq)]
pub struct Message {
    /// The job's name.
    pub job: String,
    /// `OK`, `Failed` or `Timed out`, as the window says it.
    pub status: String,
    /// How the run went, in a line: its job, its duration, its cost.
    pub summary: String,
    /// Claude's result, or the error when there is none, cut short past `MESSAGE_LIMIT` characters.
    pub result: String,
}

impl Message {
    pub fn for_run(job: &Job, rec: &RunRecord) -> Message {
        let (icon, outcome, status) = match rec.status {
            RunStatus::Ok => ("✅", "finished", "OK"),
            RunStatus::Timeout => ("⏳", "timed out", "Timed out"),
            _ => ("❌", "failed", "Failed"),
        };
        let mut summary = format!("{icon} {} {outcome}", job.name);
        let trigger = match rec.trigger {
            Trigger::Schedule => None,
            Trigger::Manual => Some("started by hand".to_string()),
            Trigger::CatchUp => Some("caught up at login".to_string()),
        };
        for fact in
            [rec.duration_s.map(duration), rec.cost_usd.map(|usd| format!("${usd:.2}")), trigger].into_iter().flatten()
        {
            summary.push_str(" · ");
            summary.push_str(&fact);
        }
        let (result, cut) = match (present(rec.result.as_deref()), present(rec.error.as_deref())) {
            (Some(result), _) => {
                let (short, cut) = shortened(result);
                (slack_text(short), cut)
            }
            (None, Some(error)) if rec.status != RunStatus::Ok => {
                let (short, cut) = shortened(error);
                (format!("```\n{short}\n```"), cut)
            }
            _ => (String::new(), false),
        };
        let result = if cut { format!("{result}\n… (cut short: the rest is in Claude Jobs)") } else { result };
        Message { job: job.name.clone(), status: status.to_string(), summary, result }
    }

    /// The whole message: `summary`, then `result` under it.
    pub fn text(&self) -> String {
        if self.result.is_empty() {
            self.summary.clone()
        } else {
            format!("{}\n{}", self.summary, self.result)
        }
    }

    /// The webhook's body: every variable as text, and the recipient's ID as `user` or `channel`. A workflow uses the
    /// ones it declares; it fails on one it declares and does not get, so they are always all there.
    pub fn payload(&self, target: &Target) -> serde_json::Value {
        let mut body = serde_json::json!({
            "text": self.text(),
            "job": self.job,
            "status": self.status,
            "summary": self.summary,
            "result": self.result,
        });
        match target {
            Target::Slack { to, recipient, .. } => body[to.key()] = recipient.clone().into(),
        }
        body
    }
}

/// Posts `body` to a webhook. Its URL is a secret, so it is never in what this returns.
fn post(url: &str, body: String) -> Result<(), String> {
    let url = url.to_string();
    // reqwest's blocking client panics inside an async runtime, which the window's commands run on: it gets a thread
    // of its own.
    thread::spawn(move || {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::blocking::Client::builder()
            .timeout(SEND_TIMEOUT)
            .build()
            .map_err(|e| format!("cannot send to Slack: {}", root_cause(&e.without_url())))?;
        let response = client.post(&url).header("Content-Type", "application/json").body(body).send().map_err(|e| {
            if e.is_timeout() {
                format!("Slack did not answer within {} s.", SEND_TIMEOUT.as_secs())
            } else {
                format!("cannot reach Slack: {}.", root_cause(&e.without_url()))
            }
        })?;
        let status = response.status();
        let answer = response.text().unwrap_or_default();
        if status.is_success() && error_in(&answer).is_none() {
            return Ok(());
        }
        Err(refusal(status.as_u16(), &answer))
    })
    .join()
    .unwrap_or_else(|_| Err("the message could not be sent.".to_string()))
}

fn root_cause(error: &(dyn std::error::Error + 'static)) -> String {
    let mut cause = error;
    while let Some(next) = cause.source() {
        cause = next;
    }
    cause.to_string()
}

/// The error in a JSON answer, `{"ok": false, "error": "…"}`, as a workflow's webhook gives it.
fn error_in(answer: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(answer).ok()?;
    if value.get("ok").and_then(serde_json::Value::as_bool) != Some(false) {
        return None;
    }
    Some(value.get("error").and_then(serde_json::Value::as_str).unwrap_or("not ok").to_string())
}

/// What Slack's answer to a refused message means, such as `404 no_service`.
fn refusal(status: u16, answer: &str) -> String {
    let said: String = error_in(answer).unwrap_or_else(|| answer.trim().to_string()).chars().take(200).collect();
    let why = match (status, said.as_str()) {
        (_, "no_service" | "no_active_hooks" | "invalid_token" | "no_service_id" | "no_team" | "team_disabled") => {
            ": the webhook was removed or turned off in Slack. Paste the workflow's current one in the action"
        }
        (_, "channel_not_found" | "channel_is_archived") => ": its channel was archived or deleted",
        (_, "action_prohibited" | "posting_to_general_channel_denied") => ": the workspace does not let it post there",
        (400, _) => {
            ": check that the workflow is published, and that it declares only variables Claude Jobs sends (see the \
             action's editor)"
        }
        (404, _) => ": the workflow was deleted, or its webhook changed. Paste its current one in the action",
        _ => "",
    };
    let said = if said.is_empty() { String::new() } else { format!(" {said}") };
    format!("Slack refused the message ({status}{said}){why}.")
}

/// `text` without the spaces around it, unless nothing is left.
fn present(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|t| !t.is_empty())
}

/// The first `MESSAGE_LIMIT` characters of `text`, and whether that left some out.
fn shortened(text: &str) -> (&str, bool) {
    match text.char_indices().nth(MESSAGE_LIMIT) {
        Some((at, _)) => (&text[..at], true),
        None => (text, false),
    }
}

/// As the window says it: "45 s", "2 min 3 s".
fn duration(seconds: u32) -> String {
    if seconds < 90 {
        format!("{seconds} s")
    } else {
        format!("{} min {} s", seconds / 60, seconds % 60)
    }
}

/// Claude's Markdown, readable in a workflow's message, which may show a variable as it is written: headings and bold
/// in Slack's single asterisks, bullets as •, links as `label (url)`, and tables, which Slack does not draw, in code
/// blocks so their columns line up. Code is left as it is.
pub fn slack_text(markdown: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut fence = false;
    let mut table = false;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            // Without the language after the backticks, which Slack would show as the first line of code.
            fence = !fence;
            out.push("```".to_string());
            continue;
        }
        if fence {
            out.push(line.to_string());
            continue;
        }
        let in_table = trimmed.starts_with('|');
        if in_table != table {
            out.push("```".to_string());
            table = in_table;
        }
        out.push(if table { line.to_string() } else { slack_line(line) });
    }
    if fence || table {
        out.push("```".to_string());
    }
    out.join("\n")
}

fn slack_line(line: &str) -> String {
    let rest = line.trim_start();
    let indent = &line[..line.len() - rest.len()];
    if let Some(title) = heading(rest) {
        return format!("{indent}*{}*", inline(&title.replace("**", "")));
    }
    for bullet in ["- ", "* ", "+ "] {
        if let Some(item) = rest.strip_prefix(bullet) {
            return format!("{indent}• {}", inline(item));
        }
    }
    format!("{indent}{}", inline(rest))
}

/// The title of a Markdown heading, `## Title`.
fn heading(line: &str) -> Option<&str> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let title = line[hashes..].strip_prefix(' ')?;
    Some(title.trim().trim_end_matches('#').trim_end())
}

/// A line's text: its code spans are left as they are.
fn inline(text: &str) -> String {
    text.split('`')
        .enumerate()
        .map(|(i, part)| if i % 2 == 1 { part.to_string() } else { links(&part.replace("**", "*").replace("~~", "~")) })
        .collect::<Vec<_>>()
        .join("`")
}

/// `[label](https://…)` as `label (https://…)`, which reads as written and links in Slack.
fn links(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some((label, tail)) = after.split_once("](") else { break };
        let Some((url, tail)) = tail.split_once(')') else { break };
        let web = ["http://", "https://", "mailto:"].iter().any(|scheme| url.starts_with(scheme));
        if label.contains('[') || !web || url.contains(char::is_whitespace) {
            out.push_str(&rest[..=open]);
            rest = after;
            continue;
        }
        out.push_str(&rest[..open]);
        if label == url {
            out.push_str(url);
        } else {
            out.push_str(&format!("{label} ({url})"));
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    fn slack(id: &str, url: &str, to: SlackTo, recipient: &str) -> Action {
        Action {
            id: id.into(),
            name: format!("{id} on Slack"),
            target: Target::Slack { webhook_url: url.into(), to, recipient: recipient.into() },
        }
    }

    const HOOK: &str = "https://hooks.slack.com/triggers/T0123ABCD/9876543210/abcdef0123456789";

    fn job() -> Job {
        serde_json::from_value(serde_json::json!({
            "id": "j", "name": "Photobook monitor", "prompt": "/j", "hour": 9, "minute": 0,
            "on_success": ["me"], "on_failure": ["ops", "me"], "on_complete": ["me", "log"]
        }))
        .expect("parses")
    }

    fn ended(status: &str, result: Option<&str>, error: Option<&str>) -> RunRecord {
        serde_json::from_value(serde_json::json!({
            "id": "20261102T080000Z", "job": "j", "trigger": "schedule", "status": status,
            "started_at": "2026-11-02T08:00:00Z", "duration_s": 123, "cost_usd": 0.554, "result": result, "error": error
        }))
        .expect("parses")
    }

    #[test]
    fn validate_tidies_an_action_and_says_what_is_wrong() {
        let input =
            Action { name: " Ops ".into(), ..slack(" ops ", &format!(" {HOOK}\n"), SlackTo::Channel, " C012AB3CD ") };
        let valid = validate(input.clone(), &[], None).expect("valid");
        assert_eq!((valid.id.as_str(), valid.name.as_str()), ("ops", "Ops"));
        assert_eq!(
            valid.target,
            Target::Slack { webhook_url: HOOK.into(), to: SlackTo::Channel, recipient: "C012AB3CD".into() }
        );
        let errors = |a: Action, existing: &[Action], editing: Option<&str>| {
            validate(a, existing, editing).expect_err("invalid")
        };
        assert_eq!(errors(input.clone(), std::slice::from_ref(&valid), None), ["An action called ops already exists."]);
        assert_eq!(
            errors(input.clone(), std::slice::from_ref(&valid), Some("other")),
            ["An action's id cannot change."]
        );
        assert!(validate(input, &[valid], Some("ops")).is_ok());
        for url in ["http://hooks.slack.com/triggers/x", "https://example.com/hook", "https://hooks.slack.com/"] {
            assert_eq!(errors(slack("a", url, SlackTo::User, "U012AB3CD"), &[], None).len(), 1, "{url}");
        }
        let blank = Action { name: " ".into(), ..slack("Bad Id", HOOK, SlackTo::User, "") };
        assert_eq!(errors(blank, &[], None).len(), 3);
    }

    #[test]
    fn the_recipient_is_a_slack_id_of_its_kind() {
        let ok = |to, id: &str| validate(slack("a", HOOK, to, id), &[], None).is_ok();
        assert!(ok(SlackTo::User, "U012AB3CD") && ok(SlackTo::User, "W0123456789"));
        assert!(ok(SlackTo::Channel, "C012AB3CD") && ok(SlackTo::Channel, "G012AB3CD"));
        assert!(!ok(SlackTo::User, "C012AB3CD"), "a channel's ID is not a user's");
        assert!(!ok(SlackTo::Channel, "U012AB3CD"), "a user's ID is not a channel's");
        for name in ["@xavier", "#claude-jobs", "xavier", "u012ab3cd", "U012", "U012AB3CD 1"] {
            assert!(!ok(SlackTo::User, name), "{name}");
        }
        let errors = validate(slack("a", HOOK, SlackTo::User, "@xavier"), &[], None).expect_err("invalid");
        assert!(errors[0].contains("Copy member ID"), "{errors:?}");
    }

    #[test]
    fn actions_keep_their_json_shape() {
        let value = serde_json::to_value(slack("ops", HOOK, SlackTo::Channel, "C012AB3CD")).expect("serializes");
        assert_eq!(
            value,
            serde_json::json!({"id": "ops", "name": "ops on Slack", "target":
                {"kind": "slack", "webhook_url": HOOK, "to": "channel", "recipient": "C012AB3CD"}})
        );
        // The variable the workflow gets the recipient in is the value's own JSON.
        for to in [SlackTo::Channel, SlackTo::User] {
            assert_eq!(serde_json::to_value(to).expect("serializes"), to.key());
        }
    }

    #[test]
    fn a_job_takes_its_success_actions_or_its_failure_actions() {
        let job = job();
        // Those on complete come on top, and one in both lists is taken once.
        assert_eq!(for_outcome(&job, RunStatus::Ok), ["me", "log"]);
        assert_eq!(for_outcome(&job, RunStatus::Failed), ["ops", "me", "log"]);
        assert_eq!(for_outcome(&job, RunStatus::Timeout), ["ops", "me", "log"]);
        assert!(for_outcome(&job, RunStatus::Interrupted).is_empty());
        assert!(for_outcome(&job, RunStatus::Running).is_empty());
    }

    #[test]
    fn the_workflow_gets_every_variable_and_the_recipient() {
        let message = Message::for_run(&job(), &ended("ok", Some("## Done\n**12** orders <checked>."), None));
        let user = message.payload(&slack("me", HOOK, SlackTo::User, "U012AB3CD").target);
        assert_eq!(
            user,
            serde_json::json!({
                "text": "✅ Photobook monitor finished · 2 min 3 s · $0.55\n*Done*\n*12* orders <checked>.",
                "job": "Photobook monitor",
                "status": "OK",
                "summary": "✅ Photobook monitor finished · 2 min 3 s · $0.55",
                "result": "*Done*\n*12* orders <checked>.",
                "user": "U012AB3CD",
            })
        );
        let channel = message.payload(&slack("ops", HOOK, SlackTo::Channel, "C012AB3CD").target);
        assert_eq!((channel["channel"].as_str(), channel.get("user")), (Some("C012AB3CD"), None));
    }

    #[test]
    fn the_message_says_how_the_run_went_then_claudes_result() {
        let message = |status, result, error| Message::for_run(&job(), &ended(status, result, error));
        let reported = message("failed", Some("BigQuery refused the query."), Some("BigQuery refused the query."));
        assert_eq!((reported.status.as_str(), reported.result.as_str()), ("Failed", "BigQuery refused the query."));
        assert_eq!(reported.summary, "❌ Photobook monitor failed · 2 min 3 s · $0.55");
        assert_eq!(message("failed", None, Some("error: boom")).result, "```\nerror: boom\n```");
        let timeout = message("timeout", None, None);
        assert_eq!((timeout.status.as_str(), timeout.result.as_str()), ("Timed out", ""));
        assert_eq!(timeout.text(), "⏳ Photobook monitor timed out · 2 min 3 s · $0.55");
        let mut by_hand = ended("ok", None, None);
        (by_hand.trigger, by_hand.duration_s, by_hand.cost_usd) = (Trigger::Manual, Some(42), None);
        assert_eq!(
            Message::for_run(&job(), &by_hand).summary,
            "✅ Photobook monitor finished · 42 s · started by hand"
        );
    }

    #[test]
    fn a_long_result_is_cut_short() {
        let long = format!("```\n{}\n```", "é".repeat(MESSAGE_LIMIT));
        let result = Message::for_run(&job(), &ended("ok", Some(&long), None)).result;
        assert!(
            result.ends_with("é\n```\n… (cut short: the rest is in Claude Jobs)"),
            "{}",
            &result[result.len() - 80..]
        );
        assert_eq!(result.matches('é').count(), MESSAGE_LIMIT - 4);
    }

    #[test]
    fn markdown_is_written_to_read_well_in_slack() {
        assert_eq!(
            slack_text("# Report #\n- **bold** and ~~gone~~\n  * nested"),
            "*Report*\n• *bold* and ~gone~\n  • nested"
        );
        assert_eq!(
            slack_text("See [the dashboard](https://x.io/d?a=1&b=2) or [https://x.io](https://x.io)."),
            "See the dashboard (https://x.io/d?a=1&b=2) or https://x.io."
        );
        assert_eq!(slack_text("[x] done, [local](file.md), [b"), "[x] done, [local](file.md), [b");
        assert_eq!(slack_text("Run `a **b** <c>` now"), "Run `a **b** <c>` now");
        assert_eq!(slack_text("```sql\nselect **1** < 2\n```"), "```\nselect **1** < 2\n```");
        assert_eq!(
            slack_text("| a | b |\n|---|---|\n| 1 | 2 |\n\nAfter"),
            "```\n| a | b |\n|---|---|\n| 1 | 2 |\n```\n\nAfter"
        );
        assert_eq!(slack_text("```\nnever closed"), "```\nnever closed\n```");
        assert_eq!(slack_text("#hashtag and 1 > 0"), "#hashtag and 1 > 0");
    }

    /// A server that answers one request with `status` and `body`, and returns the request it got.
    fn serve_once(status: &'static str, body: &'static str) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
        let url = format!("http://{}/triggers/T/1/secret", listener.local_addr().expect("address"));
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accepts");
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = stream.read(&mut buf).expect("reads");
                request.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&request).into_owned();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_string())
                        })
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    if body.len() >= length {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let response =
                format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            stream.write_all(response.as_bytes()).expect("writes");
            String::from_utf8_lossy(&request).into_owned()
        });
        (url, handle)
    }

    #[test]
    fn a_message_is_posted_as_json() {
        let (url, server) = serve_once("200 OK", r#"{"ok":true}"#);
        let action = slack("me", &url, SlackTo::User, "U012AB3CD");
        assert_eq!(test(&action), Ok(()));
        let request = server.join().expect("served");
        assert!(request.starts_with("POST /triggers/T/1/secret "), "{request}");
        assert!(request.to_ascii_lowercase().contains("content-type: application/json"), "{request}");
        let body: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").expect("body").1).expect("json");
        assert_eq!(body["user"], "U012AB3CD");
        assert_eq!(body["summary"], "👋 Test from Claude Jobs: me on Slack");
        for key in ["text", "job", "status", "result"] {
            assert!(body[key].as_str().is_some_and(|v| !v.is_empty()), "{key} in {body}");
        }
    }

    #[test]
    fn a_refused_message_says_why_without_the_webhook() {
        let (url, server) = serve_once("404 Not Found", "no_service");
        let error = post(&url, "{}".into()).expect_err("refused");
        server.join().expect("served");
        assert_eq!(
            error,
            "Slack refused the message (404 no_service): the webhook was removed or turned off in Slack. Paste the \
             workflow's current one in the action."
        );
        // A workflow says what is wrong in JSON, even with a 200.
        let (url, server) = serve_once("200 OK", r#"{"ok":false,"error":"invalid_workflow_input"}"#);
        let error = post(&url, "{}".into()).expect_err("refused");
        server.join().expect("served");
        assert_eq!(error, "Slack refused the message (200 invalid_workflow_input).");
        assert!(refusal(400, r#"{"ok":false,"error":"x"}"#).starts_with("Slack refused the message (400 x): check"));
        assert_eq!(refusal(500, ""), "Slack refused the message (500).");
        // Nothing listens on a port just freed.
        let port = TcpListener::bind("127.0.0.1:0").expect("binds").local_addr().expect("address").port();
        let error =
            post(&format!("http://127.0.0.1:{port}/triggers/T/1/secret"), "{}".into()).expect_err("unreachable");
        assert!(error.starts_with("cannot reach Slack: "), "{error}");
        assert!(!error.contains("secret"), "{error}");
    }

    #[test]
    fn the_file_is_readable_by_its_owner_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("actions.json");
        // A leftover temporary file, readable by all, does not pass its mode on.
        fs::write(path.with_extension("tmp"), "old").expect("writes");
        fs::set_permissions(path.with_extension("tmp"), fs::Permissions::from_mode(0o644)).expect("chmod");
        write_private(&path, "[]").expect("saves");
        assert_eq!(fs::metadata(&path).expect("exists").permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read_to_string(&path).expect("reads"), "[]");
    }
}
