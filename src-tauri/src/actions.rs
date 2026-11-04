//! What a job does with Slack: the actions it names in `on_success`, for a run that went well, `on_failure`, for one
//! that failed or timed out, `on_complete`, for either, and `during_run`, for the messages Claude sends itself (see
//! `mcp`). An action is defined once, in `~/.claude-jobs/actions.json`, and any job can use it. The one kind so far
//! starts a Slack workflow, one that begins "From a webhook" in Workflow Builder, with the run's result and the user
//! or channel to send it to as its variables.
//!
//! An action has a list of recipients, users or channels, and each use of it in a job can pick one of them (see
//! `ActionUse`): the same workflow can so message several people. A use with none sends no recipient, for a workflow
//! that sends where it was built to.
//!
//! Such a workflow reads flat JSON whose keys are the variables it declares, and fails when one of them is missing:
//! every message sends the same keys, see `Message::payload`.

use crate::jobs::{self, Job};
use crate::paths;
use crate::runs::{RunRecord, RunStatus, Trigger};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::thread;
use std::time::Duration;
use ts_rs::TS;

const SEND_TIMEOUT: Duration = Duration::from_secs(15);
/// What a message keeps of Claude's result: Slack folds a long one anyway, and the whole of it is in the app.
pub const MESSAGE_LIMIT: usize = 3000;
const WEBHOOK_PREFIXES: [&str; 2] = ["https://hooks.slack.com/", "https://hooks.slack-gov.com/"];

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub target: Target,
    /// Who it can send to: each job that uses it picks one.
    #[serde(default)]
    pub recipients: Vec<Recipient>,
}

impl Action {
    pub fn recipient(&self, id: &str) -> Option<&Recipient> {
        self.recipients.iter().find(|r| r.id == id)
    }
}

/// What an action sends with.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Target {
    /// A Slack workflow started by its webhook, which sends the message to the user or channel it is given.
    Slack { webhook_url: String },
}

/// Who an action's message goes to: a Slack user or channel, one of the action's, picked wherever a job uses it. Its
/// id is the action's own: two actions can each have a recipient `ops`.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Recipient {
    pub id: String,
    pub name: String,
    /// The user's or the channel's Slack ID, such as `U012AB3CD` or `C012AB3CD`. Which it is names the workflow's
    /// variable it comes in: `user` or `channel`.
    pub slack_id: String,
}

impl Recipient {
    pub fn to(&self) -> Option<SlackTo> {
        slack_kind(&self.slack_id)
    }
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

/// The recipient of a use whose recipient Claude gives, by its Slack ID: only during a run, with `send_message`.
pub const ANYONE: &str = "*";

/// An action a job uses, and which of its recipients it sends to: empty for none, `ANYONE` for whoever Claude names
/// during the run.
#[derive(Serialize, TS, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[ts(export)]
pub struct ActionUse {
    pub action: String,
    pub recipient: String,
}

impl ActionUse {
    /// The recipient it sends to, if it names one.
    pub fn recipient_of<'a>(&self, action: &'a Action) -> Option<&'a Recipient> {
        action.recipient(&self.recipient)
    }
}

impl<'de> Deserialize<'de> for ActionUse {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Stored {
            Use {
                action: String,
                #[serde(default)]
                recipient: String,
            },
            /// An action's id alone, from before recipients: the action had one user or channel, which became its
            /// recipient of the same id (see `load`).
            Id(String),
        }
        Ok(match Stored::deserialize(deserializer)? {
            Stored::Use { action, recipient } => ActionUse { action, recipient },
            Stored::Id(id) => ActionUse { action: id.clone(), recipient: id },
        })
    }
}

/// What an action did once a run ended, kept in the run's record.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ActionResult {
    pub action: String,
    /// Its name then: it may have been renamed or deleted since.
    pub name: String,
    /// Who it sent to; empty in a record from before recipients.
    #[serde(default)]
    pub recipient: String,
    /// The recipient's name then.
    #[serde(default)]
    pub recipient_name: String,
    /// Why it did not work; none when it did.
    pub error: Option<String>,
}

/// A JSON file of a list: none yet is an empty list, one that does not parse an error, never an empty list, or saving
/// would lose what it holds.
fn read_list<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Vec<T>, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{name} does not parse: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(format!("cannot read {name}: {e}")),
    }
}

/// The actions of `actions.json`. One from before recipients named a single user or channel: it becomes the action's
/// one recipient, with the action's id, which the jobs that used the action send to (see `ActionUse`).
pub fn load() -> Result<Vec<Action>, String> {
    let stored: Vec<Value> = read_list(&paths::actions_file())?;
    stored
        .into_iter()
        .map(from_stored)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("actions.json does not parse: {e}"))
}

fn from_stored(mut stored: Value) -> Result<Action, serde_json::Error> {
    let older = || -> Option<Value> {
        let target = stored.get("target")?;
        Some(serde_json::json!([{
            "id": stored.get("id")?,
            "name": stored.get("name")?,
            "slack_id": target.get("recipient")?,
        }]))
    };
    if stored.get("recipients").is_none() {
        if let Some(recipients) = older() {
            stored["recipients"] = recipients;
        }
    }
    serde_json::from_value(stored)
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
        Target::Slack { webhook_url } => Target::Slack { webhook_url: webhook_url.trim().to_string() },
    };
    let action = Action {
        id: input.id.trim().to_string(),
        name: input.name.trim().to_string(),
        target,
        recipients: input.recipients.into_iter().map(tidy).collect(),
    };
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
        Target::Slack { webhook_url } => {
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
        }
    }
    for (at, recipient) in action.recipients.iter().enumerate() {
        let who = if recipient.name.is_empty() { format!("Recipient {}", at + 1) } else { recipient.name.clone() };
        if !jobs::valid_id(&recipient.id) {
            errors.push(format!("{who}: its id is lowercase letters, digits and dashes, 40 characters at most."));
        } else if action.recipients[..at].iter().any(|r| r.id == recipient.id) {
            errors.push(format!("{who}: another recipient has the id {}.", recipient.id));
        }
        errors.extend(check_recipient(recipient).into_iter().map(|e| format!("{who}: {e}")));
    }
    if errors.is_empty() {
        Ok(action)
    } else {
        Err(errors)
    }
}

fn tidy(recipient: Recipient) -> Recipient {
    Recipient {
        id: recipient.id.trim().to_string(),
        name: recipient.name.trim().to_string(),
        slack_id: recipient.slack_id.trim().to_string(),
    }
}

/// What is wrong with a recipient's name and Slack ID.
pub fn check_recipient(recipient: &Recipient) -> Vec<String> {
    let mut errors = Vec::new();
    if recipient.name.trim().is_empty() {
        errors.push("a name is required.".to_string());
    }
    if recipient.to().is_none() {
        errors.push(
            "a Slack ID looks like U012AB3CD for a user, C012AB3CD for a channel, not a name: in Slack, open the \
             user's profile, click ⋮, then Copy member ID, or open the channel's details: it is at the bottom of About."
                .to_string(),
        );
    }
    errors
}

/// What a Slack ID is, as Slack writes it: `U012AB3CD` for a user (`W…` in Enterprise Grid), `C012AB3CD` for a
/// channel (`G…` for an older private one). None for anything else, such as a name.
fn slack_kind(id: &str) -> Option<SlackTo> {
    if !(9..=15).contains(&id.len()) || !id.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return None;
    }
    match id.chars().next()? {
        'U' | 'W' => Some(SlackTo::User),
        'C' | 'G' => Some(SlackTo::Channel),
        _ => None,
    }
}

/// What in `uses` names an action that does not exist, or a recipient the action does not have.
pub fn unknown_in<'a>(uses: impl IntoIterator<Item = &'a ActionUse>, actions: &[Action]) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    for used in uses {
        let error = match actions.iter().find(|a| a.id == used.action) {
            None => format!("No action called {}.", used.action),
            Some(action)
                if !used.recipient.is_empty()
                    && used.recipient != ANYONE
                    && action.recipient(&used.recipient).is_none() =>
            {
                format!("{} has no recipient called {}.", action.name, used.recipient)
            }
            Some(_) => continue,
        };
        if !errors.contains(&error) {
            errors.push(error);
        }
    }
    errors
}

/// The actions `job` takes for a run that ended as `status`: those for its outcome, then those for any outcome, each
/// once. None for a run still going or cut short, which has no runner left to take them.
pub fn for_outcome(job: &Job, status: RunStatus) -> Vec<ActionUse> {
    let outcome = match status {
        RunStatus::Ok => &job.on_success,
        RunStatus::Failed | RunStatus::Timeout => &job.on_failure,
        RunStatus::Running | RunStatus::Interrupted => return vec![],
    };
    let mut uses: Vec<ActionUse> = Vec::new();
    for used in outcome.iter().chain(&job.on_complete) {
        if !uses.contains(used) {
            uses.push(used.clone());
        }
    }
    uses
}

/// Takes the actions `job` names for how `rec` ended, and says what each did. One deleted since is left out.
pub fn after_run(job: &Job, rec: &RunRecord) -> Vec<ActionResult> {
    let uses = for_outcome(job, rec.status);
    if uses.is_empty() {
        return vec![];
    }
    let all = match load() {
        Ok(all) => all,
        Err(e) => {
            return uses
                .iter()
                .map(|u| ActionResult {
                    action: u.action.clone(),
                    name: u.action.clone(),
                    recipient: u.recipient.clone(),
                    recipient_name: u.recipient.clone(),
                    error: Some(e.clone()),
                })
                .collect()
        }
    };
    let message = Message::for_run(job, rec);
    uses.iter()
        .filter_map(|u| Some((u, all.iter().find(|a| a.id == u.action)?)))
        .map(|(u, action)| {
            let recipient = u.recipient_of(action);
            ActionResult {
                action: action.id.clone(),
                name: action.name.clone(),
                recipient: u.recipient.clone(),
                recipient_name: recipient.map_or_else(|| u.recipient.clone(), |r| r.name.clone()),
                error: match recipient {
                    None if !u.recipient.is_empty() => {
                        Some(format!("{} has no recipient called {}: it was deleted.", action.name, u.recipient))
                    }
                    recipient => send(action, recipient, &message).err(),
                },
            }
        })
        .collect()
}

/// Sends a message saying what the action is for, with every variable a run sends, so the workflow can be tried
/// before a run needs it.
pub fn test(action: &Action, recipient: Option<&Recipient>) -> Result<(), String> {
    let message = Message {
        job: "Claude Jobs test".to_string(),
        status: "Test".to_string(),
        summary: format!("👋 Test from Claude Jobs: {}", action.name),
        result: "When a job that uses this action ends, how the run went and Claude's result come here.".to_string(),
    };
    send(action, recipient, &message)
}

/// Sends `message` with `action`, to `recipient` if there is one: what a run's actions do when it ends, and what
/// Claude's `send_message` does.
pub fn send(action: &Action, recipient: Option<&Recipient>, message: &Message) -> Result<(), String> {
    let to = match recipient {
        Some(recipient) => match recipient.to() {
            Some(kind) => Some((kind, recipient.slack_id.as_str())),
            None => return Err(format!("{} has no Slack ID: give it one in the action.", recipient.name)),
        },
        None => None,
    };
    match &action.target {
        Target::Slack { webhook_url } => post(webhook_url, message.payload(to).to_string()),
    }
}

/// A run's message, each part a variable of the workflow.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// The job's name.
    pub job: String,
    /// `OK`, `Failed` or `Timed out`, as the window says it; `Running` for a message Claude sends during the run.
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
            Trigger::PullRequest => rec.pull_request.as_ref().map(|pr| format!("for {}#{}", pr.repo, pr.number)),
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

    /// A message Claude sends during a run, with its `send_message` tool: its Markdown as the result, under its own
    /// summary line if it gave one, else under the job's name.
    pub fn from_claude(job: &Job, summary: Option<&str>, text: &str) -> Message {
        let headline = summary.map(str::trim).filter(|s| !s.is_empty()).map_or_else(|| job.name.clone(), slack_text);
        Message {
            job: job.name.clone(),
            status: "Running".to_string(),
            summary: format!("💬 {headline}"),
            result: slack_text(text),
        }
    }

    /// The whole message: `summary`, then `result` under it.
    pub fn text(&self) -> String {
        if self.result.is_empty() {
            self.summary.clone()
        } else {
            format!("{}\n{}", self.summary, self.result)
        }
    }

    /// The webhook's body: every variable as text, and the recipient's ID, if there is one, as `user` or `channel`. A
    /// workflow uses the ones it declares; it fails on one it declares and does not get, so the text ones are always
    /// all there.
    pub fn payload(&self, to: Option<(SlackTo, &str)>) -> serde_json::Value {
        let mut body = serde_json::json!({
            "text": self.text(),
            "job": self.job,
            "status": self.status,
            "summary": self.summary,
            "result": self.result,
        });
        if let Some((kind, slack_id)) = to {
            body[kind.key()] = slack_id.into();
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
            ": check that the workflow is published, that it declares only variables Claude Jobs sends (see the \
             action's editor), and that the recipient is the kind it sends to, a user or a channel"
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

    fn slack(id: &str, url: &str) -> Action {
        Action {
            id: id.into(),
            name: format!("{id} on Slack"),
            target: Target::Slack { webhook_url: url.into() },
            recipients: vec![],
        }
    }

    fn to(id: &str, slack_id: &str) -> Recipient {
        Recipient { id: id.into(), name: format!("{id} on Slack"), slack_id: slack_id.into() }
    }

    fn used(action: &str, recipient: &str) -> ActionUse {
        ActionUse { action: action.into(), recipient: recipient.into() }
    }

    const HOOK: &str = "https://hooks.slack.com/triggers/T0123ABCD/9876543210/abcdef0123456789";

    fn job() -> Job {
        serde_json::from_value(serde_json::json!({
            "id": "j", "name": "Photobook monitor", "prompt": "/j", "hour": 9, "minute": 0,
            "on_success": [{"action": "slack", "recipient": "me"}],
            "on_failure": [{"action": "slack", "recipient": "ops"}, {"action": "slack", "recipient": "me"}],
            "on_complete": [{"action": "slack", "recipient": "me"}, {"action": "log", "recipient": "me"}]
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
        let input = Action { name: " Ops ".into(), ..slack(" ops ", &format!(" {HOOK}\n")) };
        let valid = validate(input.clone(), &[], None).expect("valid");
        assert_eq!((valid.id.as_str(), valid.name.as_str()), ("ops", "Ops"));
        assert_eq!(valid.target, Target::Slack { webhook_url: HOOK.into() });
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
            assert_eq!(errors(slack("a", url), &[], None).len(), 1, "{url}");
        }
        let blank = Action { name: " ".into(), ..slack("Bad Id", "") };
        assert_eq!(errors(blank, &[], None).len(), 3);
    }

    #[test]
    fn an_actions_recipients_are_users_or_channels_by_their_slack_id() {
        let with = |recipients: Vec<Recipient>| validate(Action { recipients, ..slack("a", HOOK) }, &[], None);
        let kind = |id: &str| with(vec![to("a", id)]).map(|a| a.recipients[0].to());
        assert_eq!(kind(" U012AB3CD "), Ok(Some(SlackTo::User)));
        assert_eq!(kind("W0123456789"), Ok(Some(SlackTo::User)));
        assert_eq!(kind("C012AB3CD"), Ok(Some(SlackTo::Channel)));
        assert_eq!(kind("G012AB3CD"), Ok(Some(SlackTo::Channel)));
        for name in ["@xavier", "#claude-jobs", "xavier", "u012ab3cd", "U012", "U012AB3CD 1", "T012AB3CD"] {
            assert!(kind(name).is_err(), "{name}");
        }
        let errors = with(vec![to("me", "@xavier")]).expect_err("invalid");
        assert!(errors[0].starts_with("me on Slack: a Slack ID looks like") && errors[0].contains("Copy member ID"));
        let twice = with(vec![to("me", "U012AB3CD"), to("me", "C012AB3CD")]).expect_err("invalid");
        assert_eq!(twice, ["me on Slack: another recipient has the id me."]);
        let blank = Recipient { name: " ".into(), ..to("Bad", "U012AB3CD") };
        assert_eq!(with(vec![blank]).expect_err("invalid").len(), 2);
    }

    #[test]
    fn actions_keep_their_json_shape() {
        let value = serde_json::to_value(Action { recipients: vec![to("ops", "C012AB3CD")], ..slack("ops", HOOK) })
            .expect("serializes");
        assert_eq!(
            value,
            serde_json::json!({"id": "ops", "name": "ops on Slack", "target": {"kind": "slack", "webhook_url": HOOK},
                "recipients": [{"id": "ops", "name": "ops on Slack", "slack_id": "C012AB3CD"}]})
        );
        // The variable the workflow gets the recipient in is the value's own JSON.
        for to in [SlackTo::Channel, SlackTo::User] {
            assert_eq!(serde_json::to_value(to).expect("serializes"), to.key());
        }
    }

    #[test]
    fn an_action_from_before_recipients_has_its_one_as_its_recipient() {
        let old = serde_json::json!({"id": "me", "name": "Private message",
            "target": {"kind": "slack", "webhook_url": HOOK, "to": "user", "recipient": "U012AB3CD"}});
        let action = from_stored(old).expect("parses");
        let me = Recipient { name: "Private message".into(), ..to("me", "U012AB3CD") };
        assert_eq!(action, Action { name: "Private message".into(), recipients: vec![me], ..slack("me", HOOK) });
        // Saved, it has the new shape, and reads the same.
        let saved = serde_json::to_value(&action).expect("serializes");
        assert_eq!(saved["target"], serde_json::json!({"kind": "slack", "webhook_url": HOOK}));
        assert_eq!(from_stored(saved).expect("parses"), action);
        // A job named the action alone: it now names that recipient too.
        let uses: Vec<ActionUse> =
            serde_json::from_value(serde_json::json!(["me", {"action": "a", "recipient": "b"}, {"action": "c"}]))
                .expect("parses");
        assert_eq!(uses, [used("me", "me"), used("a", "b"), used("c", "")]);
        assert_eq!(
            serde_json::to_value(&uses[0]).expect("serializes"),
            serde_json::json!({"action": "me", "recipient": "me"})
        );
    }

    #[test]
    fn what_a_job_uses_must_exist() {
        let actions = [Action { recipients: vec![to("me", "U012AB3CD")], ..slack("slack", HOOK) }];
        assert!(unknown_in(&[used("slack", "me")], &actions).is_empty());
        assert_eq!(
            unknown_in(
                &[used("gone", "me"), used("slack", "x"), used("slack", ""), used("slack", ANYONE), used("gone", "me")],
                &actions
            ),
            ["No action called gone.", "slack on Slack has no recipient called x."]
        );
    }

    #[test]
    fn a_job_takes_its_success_actions_or_its_failure_actions() {
        let job = job();
        // Those on complete come on top, and one in both lists is taken once.
        assert_eq!(for_outcome(&job, RunStatus::Ok), [used("slack", "me"), used("log", "me")]);
        let failed = [used("slack", "ops"), used("slack", "me"), used("log", "me")];
        assert_eq!(for_outcome(&job, RunStatus::Failed), failed);
        assert_eq!(for_outcome(&job, RunStatus::Timeout), failed);
        assert!(for_outcome(&job, RunStatus::Interrupted).is_empty());
        assert!(for_outcome(&job, RunStatus::Running).is_empty());
    }

    #[test]
    fn the_workflow_gets_every_variable_and_the_recipient() {
        let message = Message::for_run(&job(), &ended("ok", Some("## Done\n**12** orders <checked>."), None));
        let user = message.payload(Some((SlackTo::User, "U012AB3CD")));
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
        let channel = message.payload(Some((SlackTo::Channel, "C012AB3CD")));
        assert_eq!((channel["channel"].as_str(), channel.get("user")), (Some("C012AB3CD"), None));
        // With no recipient, the workflow sends where it was built to.
        let nobody = message.payload(None);
        assert_eq!((nobody.get("user"), nobody.get("channel")), (None, None));
        assert_eq!(nobody["summary"], user["summary"]);
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
        let mut for_pr = ended("ok", None, None);
        for_pr.trigger = Trigger::PullRequest;
        for_pr.pull_request = Some(
            serde_json::from_value(serde_json::json!({"repo": "o/a", "number": 21, "title": "t",
                "url": "https://github.com/o/a/pull/21", "author": "me", "head": "f", "base": "main",
                "draft": false, "created_at": "2026-11-02T08:00:00Z"}))
            .expect("parses"),
        );
        assert_eq!(
            Message::for_run(&job(), &for_pr).summary,
            "✅ Photobook monitor finished · 2 min 3 s · $0.55 · for o/a#21"
        );
    }

    #[test]
    fn a_message_claude_sends_is_its_markdown_under_the_jobs_name() {
        let message = Message::from_claude(&job(), None, "## Orders\n- **12** checked");
        let body = message.payload(Some((SlackTo::User, "U012AB3CD")));
        assert_eq!(
            body,
            serde_json::json!({
                "text": "💬 Photobook monitor\n*Orders*\n• *12* checked",
                "job": "Photobook monitor",
                "status": "Running",
                "summary": "💬 Photobook monitor",
                "result": "*Orders*\n• *12* checked",
                "user": "U012AB3CD",
            })
        );
    }

    #[test]
    fn a_message_claude_sends_can_have_its_own_summary() {
        let message = Message::from_claude(&job(), Some(" **3** orders failed "), "- order 12");
        assert_eq!(message.summary, "💬 *3* orders failed");
        assert_eq!(message.result, "• order 12");
        assert_eq!(Message::from_claude(&job(), Some(" "), "hi").summary, "💬 Photobook monitor");
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
        assert_eq!(test(&slack("me", &url), Some(&to("me", "U012AB3CD"))), Ok(()));
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
