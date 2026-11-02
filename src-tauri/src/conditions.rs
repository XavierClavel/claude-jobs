//! What a job needs before it starts: Claude Code signed in, for every job; Google Cloud signed in, for the jobs that
//! ask for it; and the command conditions, for every job or the jobs that tick them. A command condition runs a bash
//! command and looks at its exit code or what it prints. A run whose conditions are not met waits, checking again
//! every 30 s, and starts once they are.
//!
//! A condition is named by its id: `claude`, `gcloud`, or a command condition's, kept in
//! `~/.claude-jobs/command_conditions.json`. A check runs a command, so the last one is kept in
//! `~/.claude-jobs/conditions.json` for the window and `claude-jobs status`, which ask often; the runner always checks
//! again.

use crate::gcloud::{self, GcloudStatus};
use crate::jobs::{self, Job};
use crate::paths;
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use ts_rs::TS;

const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
/// Where Claude Code keeps its claude.ai login, for the default config folder the runs use.
const CREDENTIALS: &str = "Claude Code-credentials";

pub const CLAUDE_SIGNED_OUT: &str = "Claude Code is signed out: run claude, then /login.";
pub const CLAUDE_EXPIRED: &str = "Claude Code's login expired: run claude, then /login.";

/// Claude Code is signed in and its login has not expired. Every job needs it.
pub const CLAUDE: &str = "claude";
/// gcloud's credentials work, for jobs that read BigQuery or Cloud Logging.
pub const GCLOUD: &str = "gcloud";

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ConditionStatus {
    /// `claude`, `gcloud`, or a command condition's id.
    pub condition: String,
    /// What is wrong and how to fix it; none when the condition is met.
    pub problem: Option<String>,
}

impl ConditionStatus {
    pub fn met(&self) -> bool {
        self.problem.is_none()
    }
}

/// A condition of the user's own: a bash command, met when it exits or prints as `expect` says.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct CommandCondition {
    pub id: String,
    pub name: String,
    /// Run with `/bin/bash -c`, with the PATH, HOME and USER of the job it is checked for.
    pub command: String,
    pub expect: Expect,
    /// Every job waits for it, as for Claude Code; otherwise only the jobs that tick it.
    #[serde(default)]
    pub every_job: bool,
    /// What to do when it is not met, such as "Connect to the VPN". Shown with what the command did.
    #[serde(default)]
    pub hint: String,
}

/// What a command condition's command has to do.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Expect {
    /// Exit with `code`: 0 for success.
    ExitCode { code: i32 },
    /// Print `text` on stdout: all of it, or within it. Both are compared without surrounding whitespace.
    Stdout { text: String, contains: bool },
}

/// What a command condition's command did, for the editor's **Test** and **Check now**.
#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct CommandCheck {
    /// Why the condition is not met; none when it is.
    pub problem: Option<String>,
    /// None when it did not finish in time, or was stopped by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
}

pub fn load_commands() -> Result<Vec<CommandCondition>, String> {
    match fs::read_to_string(paths::command_conditions_file()) {
        // As for jobs.json: a file that does not parse is never "no conditions", or saving would lose them.
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("command_conditions.json does not parse: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(format!("cannot read command_conditions.json: {e}")),
    }
}

pub fn save_commands(conditions: &[CommandCondition]) -> Result<(), String> {
    fs::create_dir_all(paths::base()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(conditions).map_err(|e| e.to_string())?;
    let tmp = paths::command_conditions_file().with_extension("tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, paths::command_conditions_file()).map_err(|e| e.to_string())
}

/// `input` tidied up, or what is wrong with it. `editing` is the id of the condition it changes, if any.
pub fn validate_command(
    input: CommandCondition,
    existing: &[CommandCondition],
    editing: Option<&str>,
) -> Result<CommandCondition, Vec<String>> {
    let mut errors = Vec::new();
    let condition = CommandCondition {
        id: input.id.trim().to_string(),
        name: input.name.trim().to_string(),
        command: input.command.trim().to_string(),
        hint: input.hint.trim().to_string(),
        ..input
    };
    if !jobs::valid_id(&condition.id) {
        errors.push("Id: lowercase letters, digits and dashes, 40 characters at most.".to_string());
    } else if condition.id == CLAUDE || condition.id == GCLOUD {
        errors.push(format!("{} is the id of a built-in condition.", condition.id));
    }
    match editing {
        Some(current) if current != condition.id => errors.push("A condition's id cannot change.".to_string()),
        None if existing.iter().any(|c| c.id == condition.id) => {
            errors.push(format!("A condition called {} already exists.", condition.id))
        }
        _ => {}
    }
    if condition.name.is_empty() {
        errors.push("Name is required.".to_string());
    }
    if condition.command.is_empty() {
        errors.push("Command is required.".to_string());
    }
    if matches!(&condition.expect, Expect::Stdout { text, contains: true } if text.trim().is_empty()) {
        errors.push("Say what the output has to contain.".to_string());
    }
    if errors.is_empty() {
        Ok(condition)
    } else {
        Err(errors)
    }
}

/// Whether `id` names a condition a job can tick: Google Cloud, or a command condition.
pub fn is_known(id: &str, commands: &[CommandCondition]) -> bool {
    id == GCLOUD || commands.iter().any(|c| c.id == id)
}

/// What `job` needs before it starts: Claude Code signed in, the command conditions every job needs, then the
/// conditions it asked for. One that no longer exists is left out.
pub fn required(job: &Job, commands: &[CommandCondition]) -> Vec<String> {
    let mut all = vec![CLAUDE.to_string()];
    let every_job = commands.iter().filter(|c| c.every_job).map(|c| &c.id);
    let asked = job.conditions.iter().filter(|id| is_known(id, commands));
    for id in every_job.chain(asked) {
        if !all.contains(id) {
            all.push(id.clone());
        }
    }
    all
}

/// The conditions `job` needs that are not met. `fresh` checks them all now; otherwise a recent check is used.
pub fn unmet(job: &Job, fresh: bool) -> Vec<ConditionStatus> {
    // A file that does not parse holds nothing back: the window says what is wrong with it.
    let commands = load_commands().unwrap_or_default();
    let mut cache = Cache::load();
    let now = Utc::now();
    let mut changed = false;
    let mut unmet = Vec::new();
    for condition in required(job, &commands) {
        let problem = match cache.checks.get(&condition) {
            Some(check) if !fresh && check.is_recent(&condition, now) => check.problem.clone(),
            _ => {
                let problem = check(&condition, job, &cache, &commands);
                cache.checks.insert(condition.clone(), Check::new(problem.clone(), now));
                changed = true;
                problem
            }
        };
        if problem.is_some() {
            unmet.push(ConditionStatus { condition, problem });
        }
    }
    if changed {
        cache.save();
    }
    unmet
}

/// Whether Claude Code is signed in, checked now with `job`'s binary.
pub fn claude_now(job: &Job) -> ConditionStatus {
    let mut cache = Cache::load();
    let problem = check(CLAUDE, job, &cache, &[]);
    cache.checks.insert(CLAUDE.to_string(), Check::new(problem.clone(), Utc::now()));
    cache.save();
    ConditionStatus { condition: CLAUDE.to_string(), problem }
}

/// Keeps a gcloud check the window made, so the jobs show it without checking again.
pub fn record_gcloud(status: &GcloudStatus) {
    let mut cache = Cache::load();
    cache.checks.insert(GCLOUD.to_string(), Check::new(gcloud_problem(status), Utc::now()));
    cache.save();
}

/// A run found Claude Code's login expired although the checks saw nothing wrong (the login was revoked, say). It
/// stays expired until the credentials change, which signing in again does. False when there are no claude.ai
/// credentials to watch, so nothing would tell when to try again.
pub fn record_expired_login() -> bool {
    let Some(modified) = credentials_modified() else { return false };
    let mut cache = Cache::load();
    cache.expired_login = Some(modified);
    cache.checks.insert(CLAUDE.to_string(), Check::new(Some(CLAUDE_EXPIRED.to_string()), Utc::now()));
    cache.save();
    true
}

/// The last check of a command condition, and when it was made, if it is recent enough to show.
pub fn last_check(id: &str) -> Option<(Option<String>, String)> {
    let check = Cache::load().checks.remove(id)?;
    check.is_recent(id, Utc::now()).then_some((check.problem, check.checked_at))
}

/// Checks a command condition now with `path`, keeping what it found when it is a saved one.
pub fn check_command(condition: &CommandCondition, path: &str, saved: bool) -> (CommandCheck, String) {
    let found = run_command(condition, path);
    let at = Utc::now();
    if saved {
        let mut cache = Cache::load();
        cache.checks.insert(condition.id.clone(), Check::new(found.problem.clone(), at));
        cache.save();
    }
    (found, at.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn check(condition: &str, job: &Job, cache: &Cache, commands: &[CommandCondition]) -> Option<String> {
    match condition {
        CLAUDE => claude_problem(job, cache.expired_login.as_deref()),
        GCLOUD => gcloud_problem(&gcloud::status()),
        id => commands.iter().find(|c| c.id == id).and_then(|c| run_command(c, &job.path).problem),
    }
}

/// Runs a command condition's command as a job's run would, with `path`, and says whether it is met.
pub fn run_command(condition: &CommandCondition, path: &str) -> CommandCheck {
    let out = run(Command::new("/bin/bash").args(["-c", &condition.command]).env_clear().env("PATH", path));
    CommandCheck {
        problem: verdict(condition, out.as_ref()),
        exit_code: out.as_ref().and_then(|o| o.status.code()),
        stdout: out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default(),
    }
}

/// Why `out` does not meet `condition`; none when it does. No output means the command did not finish in time.
fn verdict(condition: &CommandCondition, out: Option<&Output>) -> Option<String> {
    let detail = match (out, &condition.expect) {
        (None, _) => format!("the command did not finish within {} s", CHECK_TIMEOUT.as_secs()),
        (Some(out), Expect::ExitCode { code }) => match out.status.code() {
            Some(got) if got == *code => return None,
            Some(got) => format!("the command exited with {got}, not {code}"),
            None => "the command was stopped by a signal".to_string(),
        },
        (Some(out), Expect::Stdout { text, contains }) => {
            let printed = String::from_utf8_lossy(&out.stdout);
            let (printed, text) = (printed.trim(), text.trim());
            if (*contains && printed.contains(text)) || (!*contains && printed == text) {
                return None;
            }
            match (printed.is_empty(), contains) {
                (true, _) => "the command printed nothing".to_string(),
                (false, true) => format!("the command printed {}, without {text:?}", excerpt(printed)),
                (false, false) => format!("the command printed {}, not {text:?}", excerpt(printed)),
            }
        }
    };
    let lead = if condition.hint.is_empty() {
        format!("{} is not met", condition.name)
    } else {
        condition.hint.trim_end_matches('.').to_string()
    };
    Some(format!("{lead}: {detail}."))
}

/// What a command printed, on one line and short enough for a sentence.
fn excerpt(printed: &str) -> String {
    let line = printed.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = line.chars().take(60).collect();
    if short.len() < line.len() {
        format!("{:?}", format!("{short}…"))
    } else {
        format!("{line:?}")
    }
}

pub fn gcloud_problem(status: &GcloudStatus) -> Option<String> {
    if !status.installed {
        Some("gcloud is not installed.".to_string())
    } else if !status.signed_in {
        Some("Google Cloud is signed out: sign in from Claude Jobs, or run gcloud auth login --update-adc.".to_string())
    } else {
        None
    }
}

fn claude_problem(job: &Job, expired_login: Option<&str>) -> Option<String> {
    let bin = Path::new(&job.claude);
    if !bin.is_file() {
        return Some(format!("Claude Code is not found at {}.", job.claude));
    }
    // First, whatever `auth status` says: a run was turned away with these very credentials.
    if expired_login.is_some_and(|stamp| credentials_modified().as_deref() == Some(stamp)) {
        return Some(CLAUDE_EXPIRED.to_string());
    }
    // `auth status` tells signed in from signed out, but trusts any stored login, even an expired one. A check that
    // cannot tell holds nothing back: the run itself will.
    let status: serde_json::Value =
        run(Command::new(bin).args(["auth", "status", "--json"]).env_clear().env("PATH", &job.path))
            .and_then(|out| serde_json::from_slice(&out.stdout).ok())?;
    if status.get("loggedIn").and_then(serde_json::Value::as_bool) == Some(false) {
        return Some(CLAUDE_SIGNED_OUT.to_string());
    }
    let claude_ai = status.get("authMethod").and_then(serde_json::Value::as_str) == Some("claude.ai");
    (claude_ai && login_expiry().is_some_and(|expiry| expiry <= Utc::now())).then(|| CLAUDE_EXPIRED.to_string())
}

/// When the claude.ai login can no longer be renewed: its refresh token's expiry. Only that timestamp is kept.
fn login_expiry() -> Option<DateTime<Utc>> {
    let out = run(Command::new("/usr/bin/security").args(["find-generic-password", "-s", CREDENTIALS, "-w"]))?;
    parse_login_expiry(&out.stdout)
}

fn parse_login_expiry(credentials: &[u8]) -> Option<DateTime<Utc>> {
    let value: serde_json::Value = serde_json::from_slice(credentials).ok()?;
    let millis = value.pointer("/claudeAiOauth/refreshTokenExpiresAt")?.as_i64()?;
    DateTime::from_timestamp_millis(millis)
}

/// When the claude.ai credentials last changed, from the keychain item's attributes, not its secret.
fn credentials_modified() -> Option<String> {
    let out = run(Command::new("/usr/bin/security").args(["find-generic-password", "-s", CREDENTIALS]))?;
    parse_modified(&String::from_utf8_lossy(&out.stdout))
}

fn parse_modified(attributes: &str) -> Option<String> {
    // `    "mdat"<timedate>=0x3230…00  "20261030081002Z\000"`
    let line = attributes.lines().find(|l| l.trim_start().starts_with("\"mdat\""))?;
    line.split('"').nth(3).map(str::to_string).filter(|s| !s.is_empty())
}

/// Runs a check with the HOME and USER a run gets; its output when it finished in time, whatever its exit code.
fn run(cmd: &mut Command) -> Option<Output> {
    let child = cmd
        .env("HOME", paths::home())
        .env("USER", std::env::var("USER").unwrap_or_default())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    wait_output(child, CHECK_TIMEOUT)
}

fn wait_output(mut child: std::process::Child, limit: Duration) -> Option<Output> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if child.try_wait().ok()?.is_some() {
            return child.wait_with_output().ok();
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    None
}

#[derive(Serialize, Deserialize, Default)]
struct Cache {
    #[serde(default)]
    checks: BTreeMap<String, Check>,
    /// The credentials' modification time when a run found the login expired: see `record_expired_login`.
    #[serde(default)]
    expired_login: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Check {
    problem: Option<String>,
    checked_at: String,
}

impl Check {
    fn new(problem: Option<String>, at: DateTime<Utc>) -> Self {
        Check { problem, checked_at: at.to_rfc3339_opts(SecondsFormat::Secs, true) }
    }

    /// Claude's check is quick; gcloud's prints a token, which may take a request. A command condition's is the
    /// user's own, so it is kept as briefly as Claude's.
    fn is_recent(&self, condition: &str, now: DateTime<Utc>) -> bool {
        let max_age = match condition {
            GCLOUD => TimeDelta::minutes(5),
            _ => TimeDelta::minutes(1),
        };
        DateTime::parse_from_rfc3339(&self.checked_at)
            .is_ok_and(|at| (0..=max_age.num_seconds()).contains(&(now - at.to_utc()).num_seconds()))
    }
}

impl Cache {
    fn load() -> Self {
        fs::read_to_string(paths::conditions_file())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let Ok(text) = serde_json::to_string_pretty(self) else { return };
        let _ = fs::create_dir_all(paths::base());
        // The window, `status` and the runs all write it: each through a file of its own.
        let tmp = paths::conditions_file().with_extension(format!("{}.tmp", std::process::id()));
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(&tmp, paths::conditions_file());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(conditions: Vec<&str>) -> Job {
        serde_json::from_value(serde_json::json!({
            "id": "j", "name": "J", "prompt": "/j", "hour": 9, "minute": 0, "conditions": conditions
        }))
        .expect("parses")
    }

    fn command(id: &str, command: &str, expect: Expect, every_job: bool) -> CommandCondition {
        CommandCondition {
            id: id.into(),
            name: format!("{id} up"),
            command: command.into(),
            expect,
            every_job,
            hint: String::new(),
        }
    }

    const OK: Expect = Expect::ExitCode { code: 0 };

    #[test]
    fn every_job_needs_claude_first() {
        assert_eq!(required(&job(vec![]), &[]), vec![CLAUDE]);
        assert_eq!(required(&job(vec![GCLOUD]), &[]), vec![CLAUDE, GCLOUD]);
        assert_eq!(required(&job(vec![GCLOUD, CLAUDE]), &[]), vec![CLAUDE, GCLOUD]);
    }

    #[test]
    fn command_conditions_apply_to_every_job_or_to_the_jobs_that_tick_them() {
        let commands = [command("vpn", "true", OK, true), command("db", "true", OK, false)];
        assert_eq!(required(&job(vec![]), &commands), vec![CLAUDE, "vpn"]);
        assert_eq!(required(&job(vec!["db", GCLOUD, "vpn"]), &commands), vec![CLAUDE, "vpn", "db", GCLOUD]);
        // One deleted since the job ticked it holds nothing back.
        assert_eq!(required(&job(vec!["gone"]), &commands), vec![CLAUDE, "vpn"]);
    }

    #[test]
    fn jobs_saved_before_conditions_need_only_claude() {
        let job: Job = serde_json::from_str(r#"{"id": "j", "name": "J", "prompt": "/j", "hour": 9, "minute": 0}"#)
            .expect("parses");
        assert!(job.conditions.is_empty());
        assert_eq!(required(&job, &[]), vec![CLAUDE]);
    }

    /// A `claude` whose `auth status` prints `status`.
    fn fake_claude(dir: &Path, name: &str, status: &str) -> Job {
        use std::os::unix::fs::PermissionsExt;
        let bin = dir.join(name);
        fs::write(&bin, format!("#!/bin/sh\ncat <<'EOF'\n{status}\nEOF\n")).expect("write");
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).expect("chmod");
        Job { claude: bin.display().to_string(), path: "/usr/bin:/bin".into(), ..job(vec![]) }
    }

    #[test]
    fn claude_signed_out_or_missing_holds_a_job_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let signed_out = fake_claude(dir.path(), "out", r#"{"loggedIn": false, "authMethod": "none"}"#);
        assert_eq!(claude_problem(&signed_out, None).as_deref(), Some(CLAUDE_SIGNED_OUT));
        let api_key = fake_claude(dir.path(), "key", r#"{"loggedIn": true, "authMethod": "console"}"#);
        assert_eq!(claude_problem(&api_key, None), None);
        // An older Claude Code without `auth status` cannot tell: the run will.
        let older = fake_claude(dir.path(), "old", "error: unknown command 'auth'");
        assert_eq!(claude_problem(&older, None), None);
        let missing = Job { claude: "/nowhere/claude".into(), ..job(vec![]) };
        assert!(claude_problem(&missing, None).is_some_and(|p| p.contains("not found")));
    }

    #[test]
    fn the_login_expiry_is_the_refresh_tokens() {
        let credentials = br#"{"claudeAiOauth": {"accessToken": "a", "refreshToken": "r", "expiresAt": 1793380118904,
            "refreshTokenExpiresAt": 1795865254904}, "mcpOAuth": {}}"#;
        assert_eq!(parse_login_expiry(credentials), DateTime::from_timestamp_millis(1795865254904));
        assert_eq!(parse_login_expiry(br#"{"claudeAiOauth": {"accessToken": "a"}}"#), None);
        assert_eq!(parse_login_expiry(b"not json"), None);
    }

    #[test]
    fn the_credentials_change_time_comes_from_the_attributes() {
        let attributes = "keychain: \"/Users/x/Library/Keychains/login.keychain-db\"\nattributes:\n    \"cdat\"<timedate>=0x32  \"20260327152658Z\\000\"\n    \"mdat\"<timedate>=0x32303236313033303038313030325A00  \"20261030081002Z\\000\"\n";
        assert_eq!(parse_modified(attributes).as_deref(), Some("20261030081002Z\\000"));
        assert_eq!(parse_modified("attributes:\n"), None);
    }

    #[test]
    fn a_check_is_kept_a_minute_for_claude_and_five_for_gcloud() {
        let now = Utc::now();
        let check = Check::new(None, now - TimeDelta::seconds(90));
        assert!(!check.is_recent(CLAUDE, now));
        assert!(!check.is_recent("vpn", now));
        assert!(check.is_recent(GCLOUD, now));
        assert!(!Check::new(None, now - TimeDelta::minutes(6)).is_recent(GCLOUD, now));
        // A clock set back does not keep a check forever.
        assert!(!Check::new(None, now + TimeDelta::hours(1)).is_recent(CLAUDE, now));
    }

    #[test]
    fn the_cache_keys_conditions_by_name() {
        let mut cache = Cache::default();
        cache.checks.insert(GCLOUD.into(), Check::new(Some("out".into()), Utc::now()));
        let text = serde_json::to_string(&cache).expect("serializes");
        assert!(text.contains(r#""gcloud":"#), "{text}");
        let back: Cache = serde_json::from_str(&text).expect("parses");
        assert_eq!(back.checks[GCLOUD].problem.as_deref(), Some("out"));
    }

    #[test]
    fn a_command_condition_checks_the_exit_code() {
        let path = "/usr/bin:/bin";
        assert_eq!(run_command(&command("ok", "exit 0", OK, false), path).problem, None);
        let failing = run_command(&command("vpn", "echo down; exit 7", OK, false), path);
        assert_eq!(failing.problem.as_deref(), Some("vpn up is not met: the command exited with 7, not 0."));
        assert_eq!((failing.exit_code, failing.stdout.as_str()), (Some(7), "down\n"));
        let three = command("three", "exit 3", Expect::ExitCode { code: 3 }, false);
        assert_eq!(run_command(&three, path).problem, None);
    }

    #[test]
    fn a_command_condition_checks_what_it_prints() {
        let path = "/usr/bin:/bin";
        let equals = |text: &str| Expect::Stdout { text: text.into(), contains: false };
        let contains = |text: &str| Expect::Stdout { text: text.into(), contains: true };
        // Compared without surrounding whitespace, whatever the exit code.
        assert_eq!(run_command(&command("a", "echo '  ready '; exit 1", equals("ready"), false), path).problem, None);
        assert_eq!(run_command(&command("b", "echo all systems ready", contains("ready"), false), path).problem, None);
        let other = run_command(&command("c", "echo busy", equals("ready"), false), path);
        assert_eq!(other.problem.as_deref(), Some(r#"c up is not met: the command printed "busy", not "ready"."#));
        let mut hinted = command("d", "true", contains("ready"), false);
        hinted.hint = "Start the database.".into();
        assert_eq!(
            run_command(&hinted, path).problem.as_deref(),
            Some("Start the database: the command printed nothing.")
        );
    }

    #[test]
    fn a_command_condition_runs_with_the_jobs_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp dir");
        let tool = dir.path().join("vpn-status");
        fs::write(&tool, "#!/bin/sh\necho connected\n").expect("write");
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).expect("chmod");
        let vpn = command("vpn", "vpn-status", Expect::Stdout { text: "connected".into(), contains: false }, true);
        assert_eq!(run_command(&vpn, &format!("{}:/usr/bin:/bin", dir.path().display())).problem, None);
        assert!(run_command(&vpn, "/usr/bin:/bin").problem.is_some());
    }

    #[test]
    fn a_long_output_is_cut_short_in_the_problem() {
        let long = "x".repeat(100);
        assert_eq!(excerpt(&format!("  {long}\n")), format!("{:?}", format!("{}…", "x".repeat(60))));
        assert_eq!(excerpt("two\nlines"), r#""two lines""#);
    }

    #[test]
    fn validate_tidies_a_command_condition_and_says_what_is_wrong() {
        let input = CommandCondition {
            name: " VPN ".into(),
            command: " vpn-status \n".into(),
            ..command("vpn", "", OK, false)
        };
        let valid = validate_command(input.clone(), &[], None).expect("valid");
        assert_eq!((valid.name.as_str(), valid.command.as_str()), ("VPN", "vpn-status"));
        let errors = |c: CommandCondition, existing: &[CommandCondition], editing: Option<&str>| {
            validate_command(c, existing, editing).expect_err("invalid")
        };
        assert_eq!(errors(command("gcloud", "x", OK, false), &[], None), ["gcloud is the id of a built-in condition."]);
        assert_eq!(
            errors(input.clone(), std::slice::from_ref(&valid), None),
            ["A condition called vpn already exists."]
        );
        assert_eq!(
            errors(input.clone(), std::slice::from_ref(&valid), Some("other")),
            ["A condition's id cannot change."]
        );
        assert!(validate_command(input, &[valid], Some("vpn")).is_ok());
        let blank = CommandCondition {
            name: " ".into(),
            expect: Expect::Stdout { text: " ".into(), contains: true },
            ..command("Bad Id", " ", OK, false)
        };
        assert_eq!(errors(blank, &[], None).len(), 4);
    }

    #[test]
    fn command_conditions_keep_their_json_shape() {
        let vpn = command("vpn", "vpn-status", Expect::Stdout { text: "connected".into(), contains: true }, true);
        let value = serde_json::to_value(&vpn).expect("serializes");
        assert_eq!(value["expect"], serde_json::json!({"kind": "stdout", "text": "connected", "contains": true}));
        let exit: CommandCondition = serde_json::from_value(serde_json::json!({
            "id": "db", "name": "DB", "command": "pg_isready", "expect": {"kind": "exit_code", "code": 0}
        }))
        .expect("parses");
        assert_eq!((exit.every_job, exit.hint.as_str()), (false, ""));
    }
}
