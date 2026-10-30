//! What a job needs before it starts: Claude Code signed in, for every job, and Google Cloud signed in, for the jobs
//! that ask for it. A run whose conditions are not met waits, checking again every 30 s, and starts once they are.
//!
//! A check runs a command, so the last one is kept in `~/.claude-jobs/conditions.json` for the window and
//! `claude-jobs status`, which ask often; the runner always checks again.

use crate::gcloud::{self, GcloudStatus};
use crate::jobs::Job;
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

#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Condition {
    /// Claude Code is signed in and its login has not expired. Every job needs it.
    Claude,
    /// gcloud's credentials work, for jobs that read BigQuery or Cloud Logging.
    Gcloud,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct ConditionStatus {
    pub condition: Condition,
    /// What is wrong and how to fix it; none when the condition is met.
    pub problem: Option<String>,
}

impl ConditionStatus {
    pub fn met(&self) -> bool {
        self.problem.is_none()
    }
}

/// What `job` needs before it starts: Claude Code signed in, then the conditions it asked for.
pub fn required(job: &Job) -> Vec<Condition> {
    let mut all = vec![Condition::Claude];
    for &condition in &job.conditions {
        if !all.contains(&condition) {
            all.push(condition);
        }
    }
    all
}

/// The conditions `job` needs that are not met. `fresh` checks them all now; otherwise a recent check is used.
pub fn unmet(job: &Job, fresh: bool) -> Vec<ConditionStatus> {
    let mut cache = Cache::load();
    let now = Utc::now();
    let mut changed = false;
    let mut unmet = Vec::new();
    for condition in required(job) {
        let problem = match cache.checks.get(&condition) {
            Some(check) if !fresh && check.is_recent(condition, now) => check.problem.clone(),
            _ => {
                let problem = check(condition, job, &cache);
                cache.checks.insert(condition, Check::new(problem.clone(), now));
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
    let problem = check(Condition::Claude, job, &cache);
    cache.checks.insert(Condition::Claude, Check::new(problem.clone(), Utc::now()));
    cache.save();
    ConditionStatus { condition: Condition::Claude, problem }
}

/// Keeps a gcloud check the window made, so the jobs show it without checking again.
pub fn record_gcloud(status: &GcloudStatus) {
    let mut cache = Cache::load();
    cache.checks.insert(Condition::Gcloud, Check::new(gcloud_problem(status), Utc::now()));
    cache.save();
}

/// A run found Claude Code's login expired although the checks saw nothing wrong (the login was revoked, say). It
/// stays expired until the credentials change, which signing in again does. False when there are no claude.ai
/// credentials to watch, so nothing would tell when to try again.
pub fn record_expired_login() -> bool {
    let Some(modified) = credentials_modified() else { return false };
    let mut cache = Cache::load();
    cache.expired_login = Some(modified);
    cache.checks.insert(Condition::Claude, Check::new(Some(CLAUDE_EXPIRED.to_string()), Utc::now()));
    cache.save();
    true
}

fn check(condition: Condition, job: &Job, cache: &Cache) -> Option<String> {
    match condition {
        Condition::Claude => claude_problem(job, cache.expired_login.as_deref()),
        Condition::Gcloud => gcloud_problem(&gcloud::status()),
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
    checks: BTreeMap<Condition, Check>,
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

    /// Claude's check is quick; gcloud's prints a token, which may take a request.
    fn is_recent(&self, condition: Condition, now: DateTime<Utc>) -> bool {
        let max_age = match condition {
            Condition::Claude => TimeDelta::minutes(1),
            Condition::Gcloud => TimeDelta::minutes(5),
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

    fn job(conditions: Vec<Condition>) -> Job {
        serde_json::from_value(serde_json::json!({
            "id": "j", "name": "J", "prompt": "/j", "hour": 9, "minute": 0, "conditions": conditions
        }))
        .expect("parses")
    }

    #[test]
    fn every_job_needs_claude_first() {
        assert_eq!(required(&job(vec![])), vec![Condition::Claude]);
        assert_eq!(required(&job(vec![Condition::Gcloud])), vec![Condition::Claude, Condition::Gcloud]);
        assert_eq!(
            required(&job(vec![Condition::Gcloud, Condition::Claude])),
            vec![Condition::Claude, Condition::Gcloud]
        );
    }

    #[test]
    fn jobs_saved_before_conditions_need_only_claude() {
        let job: Job = serde_json::from_str(r#"{"id": "j", "name": "J", "prompt": "/j", "hour": 9, "minute": 0}"#)
            .expect("parses");
        assert!(job.conditions.is_empty());
        assert_eq!(required(&job), vec![Condition::Claude]);
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
        assert!(!check.is_recent(Condition::Claude, now));
        assert!(check.is_recent(Condition::Gcloud, now));
        assert!(!Check::new(None, now - TimeDelta::minutes(6)).is_recent(Condition::Gcloud, now));
        // A clock set back does not keep a check forever.
        assert!(!Check::new(None, now + TimeDelta::hours(1)).is_recent(Condition::Claude, now));
    }

    #[test]
    fn the_cache_keys_conditions_by_name() {
        let mut cache = Cache::default();
        cache.checks.insert(Condition::Gcloud, Check::new(Some("out".into()), Utc::now()));
        let text = serde_json::to_string(&cache).expect("serializes");
        assert!(text.contains(r#""gcloud":"#), "{text}");
        let back: Cache = serde_json::from_str(&text).expect("parses");
        assert_eq!(back.checks[&Condition::Gcloud].problem.as_deref(), Some("out"));
    }
}
