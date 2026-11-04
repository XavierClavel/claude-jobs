//! The tools Claude Jobs gives Claude during a run: an MCP server on stdio, `claude-jobs mcp <job> <run>`, which the
//! runner passes Claude Code with `--mcp-config` when the job lets Claude send messages itself (`during_run`). Its
//! one tool so far, `send_message`, sends a message to one of the recipients the job picked, with the action it
//! picked for them, such as a Slack workflow, and keeps what it sent in `<run>.messages.jsonl`, which the runner adds
//! to the run's record once Claude is done.
//!
//! MCP over stdio is JSON-RPC 2.0, one message per line: requests get a reply on stdout, notifications none. Nothing
//! else may go to stdout.

use crate::actions::{self, Action, Message, Recipient, SlackTo, ANYONE, MESSAGE_LIMIT};

/// The longest summary line a message Claude sends may have.
const SUMMARY_LIMIT: usize = 200;
use crate::jobs::{self, Job};
use crate::runs;
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// The server's name in the run's MCP config, which Claude Code puts in its tools' names.
pub const SERVER: &str = "claude-jobs";
const SEND_MESSAGE: &str = "send_message";
/// `send_message` as Claude Code names it, for `--allowedTools`: a run cannot answer a permission prompt.
pub const SEND_MESSAGE_TOOL: &str = "mcp__claude-jobs__send_message";

/// The protocol versions whose messages this server writes the same way; the latest is answered to a client that asks
/// for another.
const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// A message Claude sent during a run, with one of its job's `during_run` actions, kept in the run's record.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct SentMessage {
    pub action: String,
    /// The action's name then: it may have been renamed or deleted since.
    pub name: String,
    /// Who it went to.
    #[serde(default)]
    pub recipient: String,
    /// The recipient's name then.
    #[serde(default)]
    pub recipient_name: String,
    pub sent_at: String,
    /// Its summary line, as Claude wrote it, if it gave one.
    #[serde(default)]
    pub summary: String,
    /// The message as Claude wrote it, in Markdown.
    pub text: String,
    /// Why it did not go out; none when it did.
    pub error: Option<String>,
}

/// The `--mcp-config` of a run: this binary, serving the run's tools.
pub fn config(job_id: &str, run_id: &str) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find the app's binary: {e}"))?;
    let server = json!({ "type": "stdio", "command": exe, "args": ["mcp", job_id, run_id] });
    Ok(json!({ "mcpServers": { SERVER: server } }).to_string())
}

/// The messages a run sent, in order, from what its server kept.
pub fn sent(job_id: &str, run_id: &str) -> Vec<SentMessage> {
    read_log(&runs::messages_file(job_id, run_id))
}

fn read_log(path: &Path) -> Vec<SentMessage> {
    let Ok(text) = fs::read_to_string(path) else { return vec![] };
    text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

/// `claude-jobs mcp <job> <run>`: serves the run's tools until Claude Code closes stdin.
pub fn serve(job_id: &str, run_id: &str) -> i32 {
    let server = match Server::load(job_id, run_id) {
        Ok(server) => server,
        Err(e) => {
            // Claude Code keeps an MCP server's stderr in its logs; the run goes on without the tools.
            eprintln!("{e}");
            return 1;
        }
    };
    let mut out = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Some(reply) = server.reply(&line, &mut actions::send) else { continue };
        if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
            break;
        }
    }
    0
}

/// Sends a message with an action, to a recipient or to where its workflow sends.
type Send<'a> = dyn FnMut(&Action, Option<&Recipient>, &Message) -> Result<(), String> + 'a;

/// Who Claude can send to, and with what: one of the job's `during_run`.
struct Destination {
    /// What Claude calls it: the recipient's id, or the action's for none; `<recipient>/<action>` for a recipient the
    /// job reaches with several actions, `<action>/anyone` for anyone.
    key: String,
    action: Action,
    recipient: Option<Recipient>,
    /// Whoever Claude gives the Slack ID of, in `slack_id`.
    anyone: bool,
}

/// What the server knows of the run it serves.
struct Server {
    job: Job,
    /// Who the job lets Claude send to, as they were when the run started.
    destinations: Vec<Destination>,
    /// Where each message sent is kept, one JSON line each.
    log: PathBuf,
}

impl Server {
    fn load(job_id: &str, run_id: &str) -> Result<Server, String> {
        if !jobs::valid_id(job_id) || !runs::valid_run_id(run_id) {
            return Err(format!("no run {job_id}/{run_id}"));
        }
        let job = jobs::find(job_id)?.ok_or_else(|| format!("no job called {job_id}"))?;
        let destinations = destinations(&job, &actions::load()?);
        Ok(Server { job, destinations, log: runs::messages_file(job_id, run_id) })
    }

    /// The reply to one line from Claude Code, if it needs one. `send` sends a message.
    fn reply(&self, line: &str, send: &mut Send) -> Option<Value> {
        let request: Value = match serde_json::from_str(line) {
            Ok(request) => request,
            Err(e) => return Some(error(Value::Null, -32700, &format!("not JSON: {e}"))),
        };
        // A notification, such as `notifications/initialized`, has no id and gets no reply.
        let id = request.get("id")?.clone();
        let params = request.get("params").cloned().unwrap_or(Value::Null);
        let result = match request.get("method").and_then(Value::as_str) {
            Some("initialize") => self.initialize(&params),
            Some("ping") => json!({}),
            Some("tools/list") => json!({ "tools": self.tools() }),
            Some("tools/call") => match params.get("name").and_then(Value::as_str) {
                Some(SEND_MESSAGE) if !self.destinations.is_empty() => {
                    self.send_message(params.get("arguments").unwrap_or(&Value::Null), send)
                }
                name => return Some(error(id, -32602, &format!("no tool called {}", name.unwrap_or("(none)")))),
            },
            Some(method) => return Some(error(id, -32601, &format!("no method called {method}"))),
            None => return Some(error(id, -32600, "a request needs a method")),
        };
        Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = asked.filter(|v| PROTOCOL_VERSIONS.contains(v)).unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": SERVER, "title": "Claude Jobs", "version": env!("CARGO_PKG_VERSION") },
            "instructions": format!(
                "Claude Jobs started this session: a scheduled run of its job \"{}\", which nobody watches. Its tools \
                 act for the job, with what was set up for it in Claude Jobs.",
                self.job.name
            ),
        })
    }

    fn tools(&self) -> Vec<Value> {
        if self.destinations.is_empty() {
            return vec![];
        }
        let keys: Vec<&str> = self.destinations.iter().map(|d| d.key.as_str()).collect();
        let list: Vec<String> = self.destinations.iter().map(|d| format!("- `{}`: {}", d.key, about(d))).collect();
        let open = self.destinations.iter().any(|d| d.anyone);
        let mut properties = json!({
            "to": { "type": "string", "enum": keys, "description": "Who to send it to." },
            "summary": {
                "type": "string",
                "description": format!(
                    "Optional: the message in one line, {SUMMARY_LIMIT} characters at most, which may be shown on its \
                     own with `text` under it, as a reply in a thread. Without it, the line is the job's name."
                ),
            },
            "text": {
                "type": "string",
                "description": format!("The message, in Markdown, {MESSAGE_LIMIT} characters at most."),
            },
        });
        let mut description = format!(
            "Sends a message now, while the run goes on, to one of those this job may send to:\n{}\n\nWrite it in \
             Markdown: it is turned into Slack's own formatting, and goes out under a line naming the job. Nobody can \
             answer it. Send only what your instructions ask for.",
            list.join("\n")
        );
        if open {
            properties["slack_id"] = json!({
                "type": "string",
                "description": "For a destination open to anyone: the Slack ID of the user (U012AB3CD) or the channel \
                                (C012AB3CD) to send to. Not a name.",
            });
            description.push_str(
                " A destination open to anyone needs `slack_id`: find a user's or a channel's ID with a Slack tool, if \
                 you have one, rather than guessing it. Its workflow sends either to users or to channels: if Slack \
                 refuses the message, the ID may be of the other kind.",
            );
        }
        vec![json!({
            "name": SEND_MESSAGE,
            "title": "Send a message",
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": ["to", "text"],
                "additionalProperties": false,
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true },
        })]
    }

    /// A tool's result: what Claude reads, and whether it failed. A failure Claude can act on is a result, not a
    /// JSON-RPC error.
    fn send_message(&self, args: &Value, send: &mut Send) -> Value {
        let key = args.get("to").and_then(Value::as_str).unwrap_or_default();
        let text = args.get("text").and_then(Value::as_str).unwrap_or_default().trim();
        let summary = args.get("summary").and_then(Value::as_str).unwrap_or_default().trim();
        let Some(destination) = self.destinations.iter().find(|d| d.key == key) else {
            let keys: Vec<&str> = self.destinations.iter().map(|d| d.key.as_str()).collect();
            return outcome(Err(format!("This job sends to no one called \"{key}\": use {}.", keys.join(" or "))));
        };
        if text.is_empty() {
            return outcome(Err("The message is empty.".to_string()));
        }
        if summary.contains('\n') || summary.chars().count() > SUMMARY_LIMIT {
            return outcome(Err(format!(
                "The summary is one line of {SUMMARY_LIMIT} characters at most: put the rest in `text`."
            )));
        }
        let length = text.chars().count();
        if length > MESSAGE_LIMIT {
            return outcome(Err(format!(
                "The message has {length} characters, more than the {MESSAGE_LIMIT} a message may have: shorten it, \
                 or send it in several."
            )));
        }
        let Destination { action, recipient, anyone, .. } = destination;
        let named;
        let recipient = if *anyone {
            let slack_id = args.get("slack_id").and_then(Value::as_str).unwrap_or_default().trim();
            named = Recipient { id: slack_id.to_string(), name: slack_id.to_string(), slack_id: slack_id.to_string() };
            if named.to().is_none() {
                return outcome(Err(format!(
                    "`{key}` needs `slack_id`, the Slack ID of a user (U012AB3CD) or of a channel (C012AB3CD), not \
                     \"{slack_id}\"."
                )));
            }
            Some(&named)
        } else {
            recipient.as_ref()
        };
        let headline = (!summary.is_empty()).then_some(summary);
        let sent = send(action, recipient, &Message::from_claude(&self.job, headline, text));
        let record = SentMessage {
            action: action.id.clone(),
            name: action.name.clone(),
            recipient: recipient.map(|r| r.id.clone()).unwrap_or_default(),
            recipient_name: recipient.map(|r| r.name.clone()).unwrap_or_default(),
            sent_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            summary: summary.to_string(),
            text: text.to_string(),
            error: sent.as_ref().err().cloned(),
        };
        if let Err(e) = keep(&self.log, &record) {
            eprintln!("cannot keep the message in the run's record: {e}");
        }
        outcome(match sent {
            Ok(()) if *anyone => Ok(format!("Sent to {} with {}.", recipient.map_or("", |r| &r.slack_id), action.name)),
            Ok(()) => Ok(format!("Sent to {}.", about(destination))),
            Err(e) => Err(format!("{} did not send it: {e}", action.name)),
        })
    }
}

/// Who the job lets Claude send to, among the actions that exist and their recipients.
fn destinations(job: &Job, actions: &[Action]) -> Vec<Destination> {
    let found: Vec<(&Action, Option<&Recipient>)> = job
        .during_run
        .iter()
        .filter_map(|u| {
            let action = actions.iter().find(|a| a.id == u.action)?;
            if u.recipient == ANYONE {
                return None;
            }
            match u.recipient_of(action) {
                // One deleted since is left out.
                None if !u.recipient.is_empty() => None,
                recipient => Some((action, recipient)),
            }
        })
        .collect();
    let base = |(action, recipient): &(&Action, Option<&Recipient>)| recipient.map_or(&action.id, |r| &r.id).clone();
    found
        .iter()
        .map(|pair| {
            let (action, recipient) = *pair;
            let shared = found.iter().filter(|other| base(other) == base(pair)).count() > 1;
            Destination {
                key: match recipient {
                    Some(r) if shared => format!("{}/{}", r.id, action.id),
                    _ => base(pair),
                },
                action: action.clone(),
                recipient: recipient.cloned(),
                anyone: false,
            }
        })
        .chain(job.during_run.iter().filter(|u| u.recipient == ANYONE).filter_map(|u| {
            let action = actions.iter().find(|a| a.id == u.action)?;
            Some(Destination {
                key: format!("{}/anyone", action.id),
                action: action.clone(),
                recipient: None,
                anyone: true,
            })
        }))
        .collect()
}

/// Who a destination is, for Claude: "Xavier, a Slack user (U012AB3CD), with Alerts", or "Alerts, wherever its Slack
/// workflow sends" for none.
fn about(d: &Destination) -> String {
    if d.anyone {
        return format!("anyone on Slack, by the Slack ID you give in `slack_id`, with {}", d.action.name);
    }
    let Some(recipient) = &d.recipient else {
        return format!("{}, wherever its Slack workflow sends", d.action.name);
    };
    let kind = match recipient.to() {
        Some(SlackTo::User) => "a Slack user",
        Some(SlackTo::Channel) => "a Slack channel",
        None => "on Slack",
    };
    format!("{}, {kind} ({}), with {}", recipient.name, recipient.slack_id, d.action.name)
}

fn outcome(result: Result<String, String>) -> Value {
    let (text, failed) = match result {
        Ok(text) => (text, false),
        Err(text) => (text, true),
    };
    json!({ "content": [{ "type": "text", "text": text }], "isError": failed })
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Adds a message to the run's log, one line, so a reader never sees half of one.
fn keep(path: &Path, record: &SentMessage) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut line = serde_json::to_string(record).map_err(|e| e.to_string())?;
    line.push('\n');
    let mut file = fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
    file.write_all(line.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Target;

    fn slack(id: &str, recipients: &[Recipient]) -> Action {
        Action {
            id: id.into(),
            name: format!("{id} workflow"),
            target: Target::Slack { webhook_url: "https://hooks.slack.com/triggers/T/1/x".into() },
            recipients: recipients.to_vec(),
        }
    }

    fn to(id: &str, name: &str, slack_id: &str) -> Recipient {
        Recipient { id: id.into(), name: name.into(), slack_id: slack_id.into() }
    }

    fn job(during_run: Value) -> Job {
        serde_json::from_value(json!({
            "id": "j", "name": "Photobook monitor", "prompt": "/j", "hour": 9, "minute": 0, "during_run": during_run
        }))
        .expect("parses")
    }

    fn server(dir: &Path) -> Server {
        let job = job(json!([{"action": "alerts", "recipient": "me"}, {"action": "alerts", "recipient": "ops"}]));
        let recipients = [to("me", "Xavier", "U012AB3CD"), to("ops", "Ops", "C012AB3CD")];
        Server {
            destinations: destinations(&job, &[slack("alerts", &recipients)]),
            job,
            log: dir.join("20261103T080000Z.messages.jsonl"),
        }
    }

    fn call(server: &Server, request: Value, send: &mut Send) -> Value {
        server.reply(&request.to_string(), send).expect("a reply")
    }

    fn never(_: &Action, _: Option<&Recipient>, _: &Message) -> Result<(), String> {
        panic!("nothing should be sent")
    }

    #[test]
    fn it_starts_as_the_client_asks_and_lists_its_tool() {
        let dir = tempfile::tempdir().expect("temp dir");
        let server = server(dir.path());
        let init = call(
            &server,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
            &mut never,
        );
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["serverInfo"]["name"], SERVER);
        assert!(init["result"]["capabilities"]["tools"].is_object());
        let newer =
            json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "2099-01-01"}});
        assert_eq!(call(&server, newer, &mut never)["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
        // A notification gets no reply.
        assert_eq!(server.reply(r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#, &mut never), None);
        assert_eq!(
            call(&server, json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}), &mut never)["result"],
            json!({})
        );

        let list = call(&server, json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"}), &mut never);
        let tools = list["result"]["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], SEND_MESSAGE);
        assert_eq!(tools[0]["inputSchema"]["properties"]["to"]["enum"], json!(["me", "ops"]));
        let about = tools[0]["description"].as_str().expect("description");
        assert!(about.contains("- `me`: Xavier, a Slack user (U012AB3CD), with alerts workflow"), "{about}");
        assert!(about.contains("- `ops`: Ops, a Slack channel (C012AB3CD), with alerts workflow"), "{about}");
        assert_eq!(SEND_MESSAGE_TOOL, format!("mcp__{SERVER}__{SEND_MESSAGE}"));
    }

    #[test]
    fn a_recipient_reached_with_several_actions_is_named_with_each() {
        let job = job(json!([
            {"action": "alerts", "recipient": "me"}, {"action": "digest", "recipient": "me"},
            {"action": "gone", "recipient": "me"}, {"action": "alerts", "recipient": "gone"},
            {"action": "digest"}
        ]));
        let me = [to("me", "Xavier", "U012AB3CD")];
        let found = destinations(&job, &[slack("alerts", &me), slack("digest", &me)]);
        let keys: Vec<&str> = found.iter().map(|d| d.key.as_str()).collect();
        // What was deleted since is left out, and a use with no recipient goes by its action.
        assert_eq!(keys, ["me/alerts", "me/digest", "digest"]);
        assert_eq!(about(&found[2]), "digest workflow, wherever its Slack workflow sends");
    }

    #[test]
    fn claude_gives_the_slack_id_of_anyone_an_open_action_sends_to() {
        let dir = tempfile::tempdir().expect("temp dir");
        let job = job(json!([{"action": "dm", "recipient": "*"}, {"action": "post", "recipient": "*"}]));
        let server = Server {
            destinations: destinations(&job, &[slack("dm", &[]), slack("post", &[])]),
            job,
            log: dir.path().join("20261103T080000Z.messages.jsonl"),
        };
        let list = call(&server, json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}), &mut never);
        let tool = &list["result"]["tools"][0];
        assert_eq!(tool["inputSchema"]["properties"]["to"]["enum"], json!(["dm/anyone", "post/anyone"]));
        assert!(tool["inputSchema"]["properties"]["slack_id"].is_object());
        assert!(tool["description"].as_str().expect("text").contains("- `dm/anyone`: anyone on Slack"));

        let mut got = Vec::new();
        let mut send = |action: &Action, recipient: Option<&Recipient>, _: &Message| {
            got.push((action.id.clone(), recipient.map(|r| r.slack_id.clone())));
            Ok(())
        };
        let request = |args: Value| json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": SEND_MESSAGE, "arguments": args}});
        let sent =
            call(&server, request(json!({"to": "post/anyone", "slack_id": " C0999ZZZZ ", "text": "hi"})), &mut send);
        assert_eq!(sent["result"]["content"][0]["text"], "Sent to C0999ZZZZ with post workflow.");
        let refused =
            call(&server, request(json!({"to": "dm/anyone", "slack_id": "@xavier", "text": "hi"})), &mut send);
        assert_eq!(refused["result"]["isError"], true);
        assert!(refused["result"]["content"][0]["text"].as_str().expect("text").contains("not \"@xavier\""));
        let missing = call(&server, request(json!({"to": "dm/anyone", "text": "hi"})), &mut send);
        assert_eq!(missing["result"]["isError"], true);
        assert_eq!(got, [("post".to_string(), Some("C0999ZZZZ".to_string()))]);
        let kept = read_log(&server.log);
        assert_eq!((kept[0].recipient.as_str(), kept[0].recipient_name.as_str()), ("C0999ZZZZ", "C0999ZZZZ"));
    }

    #[test]
    fn a_message_goes_out_to_the_recipient_and_is_kept() {
        let dir = tempfile::tempdir().expect("temp dir");
        let server = server(dir.path());
        let mut got = Vec::new();
        let mut send = |action: &Action, recipient: Option<&Recipient>, message: &Message| {
            let recipient = recipient.map(|r| r.id.clone()).unwrap_or_default();
            got.push((action.id.clone(), recipient.clone(), message.clone()));
            if recipient == "ops" {
                Err("Slack refused the message (404 no_service).".to_string())
            } else {
                Ok(())
            }
        };
        let request = |id: u32, to: &str, text: &str| {
            json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
                   "params": {"name": SEND_MESSAGE, "arguments": {"to": to, "text": text}}})
        };
        let sent = call(&server, request(1, "me", "  **12** orders checked.\n"), &mut send);
        assert_eq!(sent["result"]["isError"], false);
        assert_eq!(
            sent["result"]["content"][0]["text"],
            "Sent to Xavier, a Slack user (U012AB3CD), with alerts workflow."
        );
        let refused = call(&server, request(2, "ops", "Down"), &mut send);
        assert_eq!(refused["result"]["isError"], true);
        assert_eq!(
            refused["result"]["content"][0]["text"],
            "alerts workflow did not send it: Slack refused the message (404 no_service)."
        );
        let titled = json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": SEND_MESSAGE,
            "arguments": {"to": "me", "summary": " Tell Jade about o/a#21 ", "text": "- a column added"}}});
        assert_eq!(call(&server, titled, &mut send)["result"]["isError"], false);
        assert_eq!(got.len(), 3);
        assert_eq!(got[2].2, Message::from_claude(&server.job, Some("Tell Jade about o/a#21"), "- a column added"));
        assert_eq!((got[0].0.as_str(), got[0].1.as_str()), ("alerts", "me"));
        assert_eq!(got[0].2, Message::from_claude(&server.job, None, "**12** orders checked."));

        let kept = read_log(&server.log);
        assert_eq!(kept.len(), 3);
        assert_eq!(
            (kept[0].recipient_name.as_str(), kept[0].text.as_str(), kept[0].error.as_ref()),
            ("Xavier", "**12** orders checked.", None)
        );
        assert_eq!(kept[1].error.as_deref(), Some("Slack refused the message (404 no_service)."));

        assert_eq!(kept[2].summary, "Tell Jade about o/a#21");
    }

    #[test]
    fn what_cannot_be_sent_is_refused_before_sending() {
        let dir = tempfile::tempdir().expect("temp dir");
        let server = server(dir.path());
        let call_with = |args: Value| {
            let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": SEND_MESSAGE, "arguments": args}});
            call(&server, request, &mut never)["result"].clone()
        };
        let unknown = call_with(json!({"to": "boss", "text": "hi"}));
        assert_eq!(unknown["isError"], true);
        assert_eq!(unknown["content"][0]["text"], "This job sends to no one called \"boss\": use me or ops.");
        assert_eq!(call_with(json!({"to": "me", "text": " \n"}))["content"][0]["text"], "The message is empty.");
        let long = call_with(json!({"to": "me", "text": "é".repeat(MESSAGE_LIMIT + 1)}));
        assert!(long["content"][0]["text"].as_str().expect("text").starts_with("The message has 3001 characters"));
        for summary in ["two\nlines".to_string(), "é".repeat(SUMMARY_LIMIT + 1)] {
            let refused = call_with(json!({"to": "me", "summary": summary, "text": "hi"}));
            assert!(refused["content"][0]["text"].as_str().expect("text").starts_with("The summary is one line"));
        }
        assert!(read_log(&server.log).is_empty(), "nothing was sent, so nothing is kept");
    }

    #[test]
    fn what_it_does_not_know_is_a_protocol_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let server = server(dir.path());
        let unknown_tool =
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "rm", "arguments": {}}});
        assert_eq!(call(&server, unknown_tool, &mut never)["error"]["code"], -32602);
        let unknown_method = json!({"jsonrpc": "2.0", "id": "a", "method": "resources/list"});
        let reply = call(&server, unknown_method, &mut never);
        assert_eq!((reply["id"].as_str(), reply["error"]["code"].as_i64()), (Some("a"), Some(-32601)));
        assert_eq!(server.reply("{oops", &mut never).expect("a reply")["error"]["code"], -32700);
        // With no one to send to, there is no tool.
        let bare = Server { destinations: vec![], ..server };
        let list = call(&bare, json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}), &mut never);
        assert_eq!(list["result"]["tools"], json!([]));
    }

    #[test]
    fn the_config_runs_this_binary_for_the_run() {
        let config: Value = serde_json::from_str(&config("j", "20261103T080000Z").expect("config")).expect("json");
        let server = &config["mcpServers"][SERVER];
        assert_eq!(server["type"], "stdio");
        assert_eq!(server["args"], json!(["mcp", "j", "20261103T080000Z"]));
        assert_eq!(server["command"].as_str().map(PathBuf::from), std::env::current_exe().ok());
    }
}
