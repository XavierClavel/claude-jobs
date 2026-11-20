//! What Claude Code offers a job: the skills its prompt can start with, which the editor suggests as the prompt is
//! typed, the models it can run on, and the model and effort it gets when it picks none. Claude Code says so itself,
//! as a run in the job's folder finds them: the user's skills and the folder's, those of the plugins enabled there, and
//! the settings that apply there.

use crate::jobs::Effort;
use crate::paths;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use ts_rs::TS;

/// Claude Code answers in about a second.
const TIMEOUT: Duration = Duration::from_secs(20);

/// What the Agent SDK sends Claude Code first. It answers with its commands and models, among other things, without
/// calling the API.
const INITIALIZE: &str = r#"{"type":"control_request","request_id":"initialize","request":{"subtype":"initialize"}}"#;

/// Claude Code answers with its settings, and the model and effort the session applies from them and from `--model`.
const SETTINGS: &str = r#"{"type":"control_request","request_id":"settings","request":{"subtype":"get_settings"}}"#;

/// The model Claude Code lists for a session with no `--model` and no model setting.
const DEFAULT_MODEL: &str = "default";

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Offer {
    pub skills: Vec<Skill>,
    /// The latest of each family first, then every version.
    pub models: Vec<Model>,
    /// The version a job with no model runs on, such as `claude-opus-5-5`; none when Claude Code does not say.
    pub default_model: Option<String>,
    /// The effort the job's model runs at with no `--effort`; none when that model takes no effort, or when Claude Code
    /// does not say.
    pub default_effort: Option<Effort>,
}

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Skill {
    /// What follows the slash: `photobook-monitor`, or `quality:quality-gate` for a plugin's.
    pub name: String,
    pub description: String,
    /// What it takes after its name, such as `[low|medium|high]`; empty when nothing.
    pub argument_hint: String,
}

#[derive(Deserialize)]
struct Listed {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "argumentHint")]
    argument_hint: String,
    /// Claude Code's own commands and skills, `/clear` as well as `/code-review`: nothing tells them apart, and most of
    /// these commands mean nothing to a job.
    #[serde(default)]
    builtin: bool,
}

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Model {
    /// What `--model` takes: an alias such as `opus`, or a version's full name such as `claude-opus-4-8`.
    pub value: String,
    /// Such as `Opus 5.5`; for an alias, the version it stands for.
    pub name: String,
    /// An alias, which moves to each new version of its family, rather than one version.
    pub latest: bool,
}

#[derive(Deserialize)]
struct ListedModel {
    value: String,
    #[serde(default, rename = "resolvedModel")]
    resolved_model: String,
    #[serde(rename = "displayName")]
    display_name: String,
}

impl ListedModel {
    /// The version the model is, or the one an alias stands for.
    fn version(&self) -> &str {
        if self.resolved_model.is_empty() {
            &self.value
        } else {
            &self.resolved_model
        }
    }
}

/// What Claude Code at `claude` offers in `cwd`, run with `path` as its PATH and on the job's `model`, the way the job's
/// runs are.
pub fn list(claude: &str, path: &str, cwd: &str, model: &str) -> Result<Offer, String> {
    let cwd = if cwd.trim().is_empty() { paths::workspace() } else { paths::expand_tilde(cwd.trim()) };
    ask(&paths::expand_tilde(claude.trim()), path, &cwd, model.trim())
}

fn ask(claude: &Path, path: &str, cwd: &Path, model: &str) -> Result<Offer, String> {
    let mut child = Command::new(claude)
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"])
        // Only the lists and settings are wanted: no hook of the user's or a plugin's, no MCP server, no session left
        // behind.
        .args(["--settings", r#"{"disableAllHooks":true}"#, "--strict-mcp-config", "--no-session-persistence"])
        // The effort a model runs at by default depends on the model.
        .args(if model.is_empty() { vec![] } else { vec!["--model", model] })
        .current_dir(cwd)
        .env_clear()
        .env("PATH", path)
        .env("HOME", paths::home())
        .env("USER", std::env::var("USER").unwrap_or_default())
        .env("SHELL", paths::shell())
        .env("LANG", "en_US.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", claude.display()))?;
    // Closing its input after the requests: Claude Code answers, then exits.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{INITIALIZE}\n{SETTINGS}");
    }
    let stdout = child.stdout.take().ok_or("no output from Claude Code")?;
    let (tx, rx) = mpsc::channel();
    // Read as it comes: an answer is a single line longer than a pipe holds, which would block Claude Code.
    thread::spawn(move || {
        let mut lines = BufReader::new(stdout).lines().map_while(Result::ok);
        let _ = tx.send(read(&mut lines));
        lines.for_each(drop);
    });
    let deadline = Instant::now() + TIMEOUT;
    let offer = rx.recv_timeout(TIMEOUT).ok().flatten();
    // Left to exit by itself, since it may be saving its own settings; stopped only when it hangs.
    while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    offer.ok_or_else(|| "Claude Code did not list its skills and models.".to_string())
}

/// What Claude Code offers, from the lines it prints, read until it has answered both requests.
fn read(lines: &mut impl Iterator<Item = String>) -> Option<Offer> {
    let (mut initialized, mut settings) = (None, None);
    for line in lines {
        match answer(&line) {
            Some((id, body)) if id == "initialize" => initialized = Some(body),
            Some((id, body)) if id == "settings" => settings = Some(body),
            _ => {}
        }
        if initialized.is_some() && settings.is_some() {
            break;
        }
    }
    offer(&initialized?, &settings.unwrap_or_default())
}

/// The id of the request a line answers, and the answer, null for an error; None for any other line Claude Code prints.
fn answer(line: &str) -> Option<(String, Value)> {
    let value: Value = serde_json::from_str(line).ok()?;
    let response = value.get("response")?;
    let id = response.get("request_id")?.as_str()?.to_string();
    Some((id, response.get("response").cloned().unwrap_or_default()))
}

/// What the answers to `initialize` and `get_settings` offer.
fn offer(initialized: &Value, settings: &Value) -> Option<Offer> {
    let listed = Vec::<Listed>::deserialize(initialized.get("commands")?).ok()?;
    let skills = listed
        .into_iter()
        .filter(|c| !c.builtin)
        .map(|c| Skill { name: c.name, description: c.description, argument_hint: c.argument_hint })
        .collect();
    let listed = initialized.get("models").and_then(|m| Vec::<ListedModel>::deserialize(m).ok()).unwrap_or_default();
    // The model setting, or Claude Code's own default without one, as the version it stands for. Not what the session
    // applies, which is the job's model.
    let default_model = settings.get("effective").map(|effective| {
        let setting = effective.get("model").and_then(Value::as_str).unwrap_or(DEFAULT_MODEL);
        listed.iter().find(|m| m.value == setting).map_or(setting, ListedModel::version).to_string()
    });
    let default_effort = settings.pointer("/applied/effort").and_then(|e| Effort::deserialize(e).ok());
    Some(Offer { skills, models: models(listed), default_model, default_effort })
}

/// The aliases, then every version, those the aliases stand for first, so that a job can stay on one when its family
/// moves on.
fn models(listed: Vec<ListedModel>) -> Vec<Model> {
    let listed: Vec<ListedModel> = listed.into_iter().filter(|m| m.value != DEFAULT_MODEL).collect();
    let mut models: Vec<Model> = listed
        .iter()
        .filter(|m| m.version() != m.value)
        .map(|m| Model { value: m.value.clone(), name: m.display_name.clone(), latest: true })
        .collect();
    for m in &listed {
        if !models.iter().any(|known| !known.latest && known.value == m.version()) {
            models.push(Model { value: m.version().to_string(), name: m.display_name.clone(), latest: false });
        }
    }
    models
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    const ANSWER: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"initialize","response":{"commands":[{"name":"photobook-monitor","description":"Refresh the photobook monitor","argumentHint":""},{"name":"code-review","description":"Review the current diff","argumentHint":"[low|medium|high]"},{"name":"clear","description":"Start a new session","argumentHint":"[name]","builtin":true},{"name":"quality:quality-gate"}],"models":[{"value":"default","resolvedModel":"claude-opus-5-5","displayName":"Default (recommended)"},{"value":"opus","resolvedModel":"claude-opus-5-5","displayName":"Opus 5.5"},{"value":"haiku","resolvedModel":"claude-haiku-5-5","displayName":"Haiku 5.5"},{"value":"claude-opus-4-8","resolvedModel":"claude-opus-4-8","displayName":"Opus 4.8"},{"value":"claude-sonnet-4-6","displayName":"Sonnet 4.6"}]}}}"#;

    /// For a session started with `--model claude-opus-4-6`, under a model setting of `haiku`.
    const SETTINGS_ANSWER: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"settings","response":{"effective":{"model":"haiku","hooks":{}},"applied":{"model":"claude-opus-4-6","effort":"high"}}}}"#;

    const SETTINGS_REFUSED: &str = r#"{"type":"control_response","response":{"subtype":"error","request_id":"settings","error":"Unsupported control request subtype: get_settings"}}"#;

    fn read_all(lines: &[&str]) -> Option<Offer> {
        read(&mut lines.iter().map(|line| line.to_string()))
    }

    fn offer() -> Offer {
        read_all(&[ANSWER, SETTINGS_ANSWER]).expect("an offer")
    }

    #[test]
    fn keeps_the_skills_and_drops_claude_codes_own_commands() {
        let skills = offer().skills;
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["photobook-monitor", "code-review", "quality:quality-gate"]);
        assert_eq!(skills[1].argument_hint, "[low|medium|high]");
        assert_eq!(skills[2].description, "");
    }

    #[test]
    fn lists_the_aliases_then_every_version_and_leaves_the_default_to_the_editor() {
        let models = offer().models;
        let listed: Vec<(&str, &str, bool)> =
            models.iter().map(|m| (m.value.as_str(), m.name.as_str(), m.latest)).collect();
        assert_eq!(
            listed,
            [
                ("opus", "Opus 5.5", true),
                ("haiku", "Haiku 5.5", true),
                // The versions the aliases stand for, which Claude Code lists only through them.
                ("claude-opus-5-5", "Opus 5.5", false),
                ("claude-haiku-5-5", "Haiku 5.5", false),
                ("claude-opus-4-8", "Opus 4.8", false),
                ("claude-sonnet-4-6", "Sonnet 4.6", false),
            ]
        );
        // A Claude Code that lists no models still lists its skills.
        let without = ANSWER.replace(r#""models":["#, r#""unused":["#);
        assert_eq!(read_all(&[&without, SETTINGS_ANSWER]).expect("an offer").models, vec![]);
    }

    #[test]
    fn the_default_model_is_the_settings_version_and_the_default_effort_the_jobs_models() {
        let offer = offer();
        // The setting, not the session's `--model`.
        assert_eq!(offer.default_model.as_deref(), Some("claude-haiku-5-5"));
        assert_eq!(offer.default_effort, Some(Effort::High));
        let with = |effective: &str, applied: &str| {
            let settings = SETTINGS_ANSWER
                .replace(r#"{"model":"haiku","hooks":{}}"#, effective)
                .replace(r#"{"model":"claude-opus-4-6","effort":"high"}"#, applied);
            read_all(&[ANSWER, &settings]).expect("an offer")
        };
        // No model setting: Claude Code's own default.
        assert_eq!(with("{}", "{}").default_model.as_deref(), Some("claude-opus-5-5"));
        // A setting Claude Code does not list, as it is.
        assert_eq!(with(r#"{"model":"opus[1m]"}"#, "{}").default_model.as_deref(), Some("opus[1m]"));
        // A model that takes no effort.
        assert_eq!(with("{}", r#"{"model":"claude-haiku-4-5-20251001","effort":null}"#).default_effort, None);
    }

    #[test]
    fn a_claude_code_that_does_not_give_its_settings_still_offers_its_skills_and_models() {
        let offer = read_all(&[ANSWER, SETTINGS_REFUSED]).expect("an offer");
        assert_eq!((offer.skills.len(), offer.models.len()), (3, 6));
        assert_eq!((offer.default_model, offer.default_effort), (None, None));
    }

    #[test]
    fn other_lines_are_not_answers() {
        assert_eq!(answer(r#"{"type":"system","subtype":"hook_started"}"#), None);
        assert_eq!(answer(r#"{"type":"control_response","response":{"subtype":"error","error":"no"}}"#), None);
        assert_eq!(answer("not json"), None);
        assert_eq!(read_all(&[r#"{"type":"system","subtype":"hook_started"}"#, SETTINGS_ANSWER]), None);
    }

    /// A stand-in for Claude Code that keeps its arguments, reads the requests, prints a line before its answers, and
    /// checks where it runs.
    fn fake_claude(dir: &Path) -> std::path::PathBuf {
        let bin = dir.join("claude");
        let quoted = |text: &str| text.replace('\'', r"'\''");
        let script = format!(
            "#!/bin/sh\necho \"$@\" > '{dir}/args'\n\
             read request\ncase \"$request\" in *initialize*) ;; *) exit 1 ;; esac\n\
             read request\ncase \"$request\" in *get_settings*) ;; *) exit 1 ;; esac\n\
             [ \"$PWD\" = '{dir}' ] || exit 1\n\
             echo '{{\"type\":\"system\",\"subtype\":\"hook_started\"}}'\necho '{answer}'\necho '{settings}'\n",
            dir = dir.display(),
            answer = quoted(ANSWER),
            settings = quoted(SETTINGS_ANSWER),
        );
        fs::write(&bin, script).expect("write");
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).expect("chmod");
        bin
    }

    #[test]
    fn asks_claude_code_in_the_jobs_folder_on_the_jobs_model() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let dir = fs::canonicalize(tmp.path()).expect("canonical");
        let bin = fake_claude(&dir);
        let args = || fs::read_to_string(dir.join("args")).expect("args");
        let model = "claude-opus-4-6";
        assert_eq!(ask(&bin, "/usr/bin:/bin", &dir, model), Ok(offer()));
        assert!(args().contains(&format!("--model {model}")), "{}", args());
        assert_eq!(ask(&bin, "/usr/bin:/bin", &dir, ""), Ok(offer()));
        assert!(!args().contains("--model"), "{}", args());
        assert!(ask(&bin, "/usr/bin:/bin", Path::new("/"), "").is_err());
        assert!(ask(&dir.join("missing"), "/usr/bin:/bin", &dir, "").is_err());
    }
}
