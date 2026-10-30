//! The skills a job's prompt can start with, which the editor suggests as the prompt is typed. Claude Code lists them
//! itself, as a run in the job's folder finds them: the user's and the folder's, and those of the plugins enabled
//! there.

use crate::paths;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use ts_rs::TS;

/// Claude Code answers in about a second.
const TIMEOUT: Duration = Duration::from_secs(20);

/// What the Agent SDK sends Claude Code first. It answers with its commands, among other things, without calling the
/// API.
const INITIALIZE: &str = r#"{"type":"control_request","request_id":"skills","request":{"subtype":"initialize"}}"#;

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

/// The skills Claude Code at `claude` finds in `cwd`, run with `path` as its PATH, the way the job's runs are.
pub fn list(claude: &str, path: &str, cwd: &str) -> Result<Vec<Skill>, String> {
    let cwd = if cwd.trim().is_empty() { paths::home() } else { paths::expand_tilde(cwd.trim()) };
    ask(&paths::expand_tilde(claude.trim()), path, &cwd)
}

fn ask(claude: &Path, path: &str, cwd: &Path) -> Result<Vec<Skill>, String> {
    let mut child = Command::new(claude)
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"])
        // Only the list is wanted: no hook of the user's or a plugin's, no MCP server, no session left behind.
        .args(["--settings", r#"{"disableAllHooks":true}"#, "--strict-mcp-config", "--no-session-persistence"])
        .current_dir(cwd)
        .env_clear()
        .env("PATH", path)
        .env("HOME", paths::home())
        .env("USER", std::env::var("USER").unwrap_or_default())
        .env("SHELL", "/bin/zsh")
        .env("LANG", "en_US.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", claude.display()))?;
    // Closing its input after the request: Claude Code answers, then exits.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{INITIALIZE}");
    }
    let stdout = child.stdout.take().ok_or("no output from Claude Code")?;
    let (tx, rx) = mpsc::channel();
    // Read as it comes: the answer is a single line longer than a pipe holds, which would block Claude Code.
    thread::spawn(move || {
        let mut lines = BufReader::new(stdout).lines().map_while(Result::ok);
        let _ = tx.send(lines.find_map(|line| parse(&line)));
        lines.for_each(drop);
    });
    let deadline = Instant::now() + TIMEOUT;
    let skills = rx.recv_timeout(TIMEOUT).ok().flatten();
    // Left to exit by itself, since it may be saving its own settings; stopped only when it hangs.
    while Instant::now() < deadline && matches!(child.try_wait(), Ok(None)) {
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    skills.ok_or_else(|| "Claude Code did not list its skills.".to_string())
}

/// The skills in Claude Code's answer to `initialize`; None for any other line it prints.
fn parse(line: &str) -> Option<Vec<Skill>> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let listed = Vec::<Listed>::deserialize(value.pointer("/response/response/commands")?).ok()?;
    Some(
        listed
            .into_iter()
            .filter(|c| !c.builtin)
            .map(|c| Skill { name: c.name, description: c.description, argument_hint: c.argument_hint })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    const ANSWER: &str = r#"{"type":"control_response","response":{"subtype":"success","request_id":"skills","response":{"commands":[{"name":"photobook-monitor","description":"Refresh the photobook monitor","argumentHint":""},{"name":"code-review","description":"Review the current diff","argumentHint":"[low|medium|high]"},{"name":"clear","description":"Start a new session","argumentHint":"[name]","builtin":true},{"name":"quality:quality-gate"}],"models":[]}}}"#;

    #[test]
    fn keeps_the_skills_and_drops_claude_codes_own_commands() {
        let skills = parse(ANSWER).expect("an answer");
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["photobook-monitor", "code-review", "quality:quality-gate"]);
        assert_eq!(skills[1].argument_hint, "[low|medium|high]");
        assert_eq!(skills[2].description, "");
    }

    #[test]
    fn other_lines_are_not_the_answer() {
        assert_eq!(parse(r#"{"type":"system","subtype":"hook_started"}"#), None);
        assert_eq!(parse(r#"{"type":"control_response","response":{"subtype":"error","error":"no"}}"#), None);
        assert_eq!(parse("not json"), None);
    }

    /// A stand-in for Claude Code that reads the request, prints a line before its answer, and checks where it runs.
    fn fake_claude(dir: &Path) -> std::path::PathBuf {
        let bin = dir.join("claude");
        let answer = ANSWER.replace('\'', r"'\''");
        let script = format!(
            "#!/bin/sh\nread request\ncase \"$request\" in *initialize*) ;; *) exit 1 ;; esac\n\
             [ \"$PWD\" = '{}' ] || exit 1\n\
             echo '{{\"type\":\"system\",\"subtype\":\"hook_started\"}}'\necho '{answer}'\n",
            dir.display()
        );
        fs::write(&bin, script).expect("write");
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).expect("chmod");
        bin
    }

    #[test]
    fn asks_claude_code_in_the_jobs_folder() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let dir = fs::canonicalize(tmp.path()).expect("canonical");
        let bin = fake_claude(&dir);
        let skills = ask(&bin, "/usr/bin:/bin", &dir).expect("skills");
        assert_eq!(skills.len(), 3);
        assert!(ask(&bin, "/usr/bin:/bin", Path::new("/")).is_err());
        assert!(ask(&dir.join("missing"), "/usr/bin:/bin", &dir).is_err());
    }
}
