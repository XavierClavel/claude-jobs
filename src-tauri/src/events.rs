//! Jobs a trigger starts instead of a schedule: a pull request opened, or merged, in a GitHub repository.
//!
//! GitHub cannot reach the Mac, so while such a job is enabled a launchd agent, `com.claude-jobs.app.events`, calls
//! `claude-jobs poll` every 5 minutes: it lists the latest pull requests of each job's repositories with the GitHub CLI,
//! and starts the job for one opened, or merged, since the job began watching, with the pull request after its prompt.
//! A job runs once at a time, so one that comes while it runs, or waits for its conditions, starts at the first check
//! after that.
//!
//! What each job has seen is kept in `~/.claude-jobs/events.json`: when it began watching each repository, the pull
//! requests it started for, and its last check, which the window shows. A paused job forgets it, so resuming it does
//! not start it for what was opened meanwhile.

use crate::jobs::{self, Job};
use crate::runs::{self, Trigger};
use crate::{paths, runner};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use ts_rs::TS;

/// The latest pull requests a check reads per repository: more than are opened between two checks, or in a night off.
const LIST_LIMIT: &str = "30";
const FIELDS: &str = "number,title,url,author,createdAt,headRefName,baseRefName,isDraft";
/// A merged pull request's, with when it was merged.
const MERGED_FIELDS: &str = "number,title,url,author,createdAt,headRefName,baseRefName,isDraft,mergedAt";
const GH_TIMEOUT: Duration = Duration::from_secs(20);
/// Listing every repository takes a request per hundred: a few seconds for a few hundred.
const REPOS_TIMEOUT: Duration = Duration::from_secs(90);
/// How long the editor's list of repositories is kept before GitHub is asked again.
const REPOS_FRESH: Duration = Duration::from_secs(600);
/// Pull requests a job remembers starting for: far more than a check reads, so none comes back as new.
const KEEP_STARTED: usize = 500;

pub const GH_MISSING: &str = "Watching pull requests needs the GitHub CLI: run brew install gh, then gh auth login.";
pub const GH_SIGNED_OUT: &str = "The GitHub CLI is signed out: run gh auth login.";

/// What starts a job instead of its schedule: its trigger.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Event {
    pub kind: EventKind,
    /// The repositories it watches, each `owner/name`.
    pub repos: Vec<String>,
    /// Only the pull requests you open, as the GitHub CLI's account; otherwise anyone's.
    #[serde(default = "mine_by_default")]
    pub mine: bool,
}

/// The triggers there are.
#[derive(Serialize, Deserialize, TS, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EventKind {
    /// A pull request opened in one of the repositories.
    PullRequestOpened,
    /// A pull request merged in one of them.
    PullRequestMerged,
}

impl EventKind {
    /// What happened to the pull request, in a word.
    fn verb(self) -> &'static str {
        match self {
            EventKind::PullRequestOpened => "opened",
            EventKind::PullRequestMerged => "merged",
        }
    }
}

fn mine_by_default() -> bool {
    true
}

/// The pull request a run was started for, kept in its record.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct PullRequest {
    /// `owner/name`, as the job names it.
    pub repo: String,
    pub number: u32,
    pub title: String,
    pub url: String,
    /// The login of whoever opened it.
    pub author: String,
    /// Its branch.
    pub head: String,
    /// The branch it goes into.
    pub base: String,
    pub draft: bool,
    pub created_at: String,
    /// When it was merged, for a run a merge started.
    #[serde(default)]
    pub merged_at: Option<String>,
}

impl PullRequest {
    /// When what starts a job of `kind` happened to it.
    fn happened_at(&self, kind: EventKind) -> Option<&str> {
        match kind {
            EventKind::PullRequestOpened => Some(&self.created_at),
            EventKind::PullRequestMerged => self.merged_at.as_deref(),
        }
    }
}

/// A repository your GitHub account can see, for the editor's picker.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct Repo {
    /// `owner/name`
    pub name: String,
    pub private: bool,
    /// When it was last pushed to: the list is newest first.
    pub pushed_at: Option<String>,
    /// The picture of its owner, a user or an organization, on GitHub's avatar server.
    #[serde(default)]
    pub owner_avatar: Option<String>,
}

/// The last list of repositories, with the PATH it was made with and when.
static REPOS: Mutex<Option<(String, Instant, Vec<Repo>)>> = Mutex::new(None);

/// Every repository the GitHub CLI's account owns, collaborates on, or can see through its organizations, most
/// recently pushed first. Kept 10 minutes, since a few hundred take seconds.
pub fn list_repos(path: &str) -> Result<Vec<Repo>, String> {
    if let Some((cached, at, repos)) = REPOS.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if cached == path && at.elapsed() < REPOS_FRESH {
            return Ok(repos.clone());
        }
    }
    let gh = gh_bin(path).ok_or_else(|| GH_MISSING.to_string())?;
    let args = [
        "api",
        "--paginate",
        "user/repos?per_page=100&sort=pushed&affiliation=owner,collaborator,organization_member",
        "--jq",
        ".[] | {name: .full_name, private, pushed_at, owner_avatar: .owner.avatar_url}",
    ];
    let lines = gh_output_within(&gh, path, "", &args, REPOS_TIMEOUT)?;
    let repos = parse_repos(&lines)?;
    *REPOS.lock().unwrap_or_else(|p| p.into_inner()) = Some((path.to_string(), Instant::now(), repos.clone()));
    Ok(repos)
}

/// One JSON object per line, as `--jq` prints them, newest push first across the pages.
fn parse_repos(lines: &str) -> Result<Vec<Repo>, String> {
    let mut repos: Vec<Repo> = lines
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("the GitHub CLI's list of repositories does not parse: {e}"))?;
    repos.sort_by(|a, b| b.pushed_at.cmp(&a.pushed_at));
    repos.dedup_by(|a, b| a.name == b.name);
    Ok(repos)
}

/// How watching for a job's event goes, for the window and `claude-jobs status`.
#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct WatchStatus {
    /// The last check; none before the first.
    pub checked_at: Option<String>,
    /// Why the last check failed; none when it worked.
    pub problem: Option<String>,
    /// Pull requests found that wait for the job's current run to end.
    pub pending: u32,
}

/// A job's event tidied up, or what is wrong with it: repositories pasted as links become `owner/name`.
pub fn validate(event: Event, path: &str) -> Result<Event, Vec<String>> {
    let mut errors = Vec::new();
    let mut tidy: Vec<String> = Vec::new();
    for repo in event.repos.iter().map(|r| repo_name(r)).filter(|r| !r.is_empty()) {
        if !valid_repo(&repo) {
            errors
                .push(format!("{repo} is not a repository: write it as owner/name, such as XavierClavel/claude-jobs."));
        } else if !tidy.iter().any(|r| r.eq_ignore_ascii_case(&repo)) {
            tidy.push(repo);
        }
    }
    if tidy.is_empty() && errors.is_empty() {
        errors.push("Add the repositories to watch, as owner/name.".to_string());
    }
    if gh_bin(path).is_none() {
        errors.push(GH_MISSING.to_string());
    }
    if errors.is_empty() {
        Ok(Event { repos: tidy, ..event })
    } else {
        Err(errors)
    }
}

/// `owner/name` from what was typed or pasted: `owner/name`, or a link to the repository or to a page in it.
fn repo_name(given: &str) -> String {
    let given = given.trim();
    let hosted = ["https://github.com/", "http://github.com/", "github.com/", "git@github.com:"]
        .iter()
        .find_map(|host| given.strip_prefix(host));
    let name = match hosted {
        // A link may go on to a page of the repository, such as its pull requests.
        Some(rest) => rest.split('/').filter(|s| !s.is_empty()).take(2).collect::<Vec<_>>().join("/"),
        None => given.trim_end_matches('/').to_string(),
    };
    name.strip_suffix(".git").unwrap_or(&name).to_string()
}

/// GitHub's rules: an owner is letters, digits and dashes; a repository's name may also have dots and underscores.
/// Opens a repository's page on GitHub in the default browser, or one of its pull requests' with `number`.
pub fn open_on_github(repo: &str, number: Option<u32>) -> Result<(), String> {
    if !valid_repo(repo) {
        return Err(format!("{repo} is not a GitHub repository, owner/name."));
    }
    let url = match number {
        Some(n) => format!("https://github.com/{repo}/pull/{n}"),
        None => format!("https://github.com/{repo}"),
    };
    let status = Command::new("open").arg(&url).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("Cannot open {url}.")) }
}

fn valid_repo(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else { return false };
    (1..=39).contains(&owner.len())
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && (1..=100).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && name != "."
        && name != ".."
}

/// Asks GitHub about each repository `job` watches that `previous`, the job as it was saved, did not, and names them
/// as GitHub does. A typo or a repository the account cannot see shows when the job is saved, not at the next check.
pub fn confirm(mut job: Job, previous: Option<&Job>) -> Result<Job, Vec<String>> {
    let watched: Vec<String> = previous.and_then(|p| p.event.as_ref()).map(|e| e.repos.clone()).unwrap_or_default();
    let path = job.path.clone();
    let Some(Event { repos, .. }) = job.event.as_mut() else { return Ok(job) };
    let gh = gh_bin(&path).ok_or_else(|| vec![GH_MISSING.to_string()])?;
    let mut errors = Vec::new();
    for repo in repos.iter_mut().filter(|r| !watched.contains(*r)) {
        let name = repo.clone();
        let args = ["repo", "view", name.as_str(), "--json", "nameWithOwner", "--jq", ".nameWithOwner"];
        match gh_output(&gh, &path, &name, &args) {
            Ok(canonical) if !canonical.trim().is_empty() => *repo = canonical.trim().to_string(),
            Ok(_) => errors.push(format!("{repo}: GitHub said nothing about it.")),
            Err(problem) => errors.push(problem),
        }
    }
    errors.dedup();
    if errors.is_empty() {
        Ok(job)
    } else {
        Err(errors)
    }
}

/// The prompt of a run a pull request started: the job's, then the pull request, which a skill gets as its arguments.
pub fn prompt(job_prompt: &str, pr: &PullRequest) -> String {
    // Only a run a merge started has the time of the merge.
    let (verb, then) = match &pr.merged_at {
        Some(at) => ("merged", format!(", merged at {at}")),
        None => ("opened", if pr.draft { ", as a draft".to_string() } else { String::new() }),
    };
    format!(
        "{job_prompt}\n\nPull request {}#{} {verb}: {}\n{}\nBy {}, from {} into {}{then}.",
        pr.repo, pr.number, pr.title, pr.url, pr.author, pr.head, pr.base
    )
}

pub fn gh_bin(path: &str) -> Option<PathBuf> {
    path.split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join("gh"))
        .chain(["/opt/homebrew/bin/gh", "/usr/local/bin/gh"].map(PathBuf::from))
        .find(|p| p.is_file())
}

/// Runs the GitHub CLI as a job's run would, and returns what it printed, or what went wrong in a sentence.
fn gh_output(gh: &Path, path: &str, repo: &str, args: &[&str]) -> Result<String, String> {
    gh_output_within(gh, path, repo, args, GH_TIMEOUT)
}

fn gh_output_within(gh: &Path, path: &str, repo: &str, args: &[&str], limit: Duration) -> Result<String, String> {
    let child = Command::new(gh)
        .args(args)
        .env_clear()
        .env("PATH", path)
        .env("HOME", paths::home())
        .env("USER", std::env::var("USER").unwrap_or_default())
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run the GitHub CLI: {e}"))?;
    let out =
        wait_output(child, limit).ok_or_else(|| format!("GitHub did not answer within {} s.", limit.as_secs()))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(gh_problem(repo, &String::from_utf8_lossy(&out.stderr)))
    }
}

/// Reads the child's output as it comes, so a long answer cannot fill the pipe and hold it; none past `limit`.
fn wait_output(mut child: Child, limit: Duration) -> Option<Output> {
    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    let out = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    Some(Output { status, stdout: out.join().unwrap_or_default(), stderr: err.join().unwrap_or_default() })
}

/// What the GitHub CLI's error means, in a sentence that says what to do.
fn gh_problem(repo: &str, stderr: &str) -> String {
    let said = stderr.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("it failed");
    if stderr.contains("gh auth login") || stderr.contains("not logged in") {
        GH_SIGNED_OUT.to_string()
    } else if ["Bad credentials", "HTTP 401", "401 Unauthorized"].iter().any(|s| stderr.contains(s)) {
        "The GitHub CLI's login no longer works: run gh auth login.".to_string()
    } else if stderr.contains("Could not resolve to a Repository") || stderr.contains("HTTP 404") {
        format!("{repo}: no such repository, or your GitHub account cannot see it.")
    } else if ["error connecting to", "no such host", "dial tcp", "i/o timeout"].iter().any(|s| stderr.contains(s)) {
        format!("Cannot reach GitHub: {said}")
    } else if repo.is_empty() {
        format!("The GitHub CLI failed: {said}")
    } else {
        format!("{repo}: {said}")
    }
}

/// A pull request as `gh pr list --json` gives it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listed {
    number: u32,
    title: String,
    url: String,
    #[serde(default)]
    author: Option<Login>,
    created_at: String,
    head_ref_name: String,
    base_ref_name: String,
    #[serde(default)]
    is_draft: bool,
    #[serde(default)]
    merged_at: Option<String>,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

fn parse_list(repo: &str, json: &str) -> Result<Vec<PullRequest>, String> {
    let listed: Vec<Listed> =
        serde_json::from_str(json).map_err(|e| format!("{repo}: the GitHub CLI's answer does not parse: {e}"))?;
    Ok(listed
        .into_iter()
        .map(|l| PullRequest {
            repo: repo.to_string(),
            number: l.number,
            title: l.title,
            url: l.url,
            author: l.author.map(|a| a.login).unwrap_or_default(),
            head: l.head_ref_name,
            base: l.base_ref_name,
            draft: l.is_draft,
            created_at: l.created_at,
            merged_at: l.merged_at,
        })
        .collect())
}

/// The latest pull requests of the event's repositories, those opened or those merged, and what went wrong with
/// those that could not be read.
fn list_all(job: &Job, event: &Event) -> (Vec<PullRequest>, Option<String>) {
    let Some(gh) = gh_bin(&job.path) else { return (vec![], Some(GH_MISSING.to_string())) };
    let (state, fields) = match event.kind {
        EventKind::PullRequestOpened => ("all", FIELDS),
        EventKind::PullRequestMerged => ("merged", MERGED_FIELDS),
    };
    let mut found = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    for repo in &event.repos {
        let mut args = vec!["pr", "list", "--repo", repo, "--state", state, "--limit", LIST_LIMIT, "--json", fields];
        if event.mine {
            args.extend(["--author", "@me"]);
        }
        match gh_output(&gh, &job.path, repo, &args).and_then(|json| parse_list(repo, &json)) {
            Ok(prs) => found.extend(prs),
            // Signed out says the same for every repository.
            Err(problem) if !problems.contains(&problem) => problems.push(problem),
            Err(_) => {}
        }
    }
    (found, (!problems.is_empty()).then(|| problems.join(" ")))
}

/// What `events.json` keeps per job.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
struct Watch {
    /// What it watches for: when that changes, it watches from then on.
    #[serde(default)]
    kind: Option<EventKind>,
    /// Whether it watches only your pull requests: when that changes, it watches from then on.
    #[serde(default)]
    mine: Option<bool>,
    /// When it began watching each repository: a pull request opened before is not new.
    #[serde(default)]
    since: BTreeMap<String, String>,
    /// The links of the pull requests it started for, oldest first.
    #[serde(default)]
    started: Vec<String>,
    #[serde(default)]
    checked_at: Option<String>,
    #[serde(default)]
    problem: Option<String>,
    #[serde(default)]
    pending: u32,
}

impl Watch {
    fn watches(&self, event: &Event) -> bool {
        self.kind == Some(event.kind) && self.mine == Some(event.mine)
    }

    /// Watches the event's repositories from `now` on, those it watched already from when it began.
    fn follow(&mut self, event: &Event, now: &str) {
        if !self.watches(event) {
            // What happened before is not new when it watches for something else, or for anyone's pull requests.
            self.since.clear();
            self.started.clear();
            (self.kind, self.mine) = (Some(event.kind), Some(event.mine));
        }
        let repos = &event.repos;
        self.since.retain(|repo, _| repos.contains(repo));
        for repo in repos {
            self.since.entry(repo.clone()).or_insert_with(|| now.to_string());
        }
    }

    /// The pull requests of `found` to start the job for, oldest first: opened, or merged, since it began watching
    /// their repository, and not started for yet.
    fn new_ones(&self, found: Vec<PullRequest>, event: &Event) -> Vec<PullRequest> {
        if !self.watches(event) {
            return vec![];
        }
        let kind = event.kind;
        let mut new: Vec<PullRequest> = found
            .into_iter()
            .filter(|pr| {
                let since = self.since.get(&pr.repo).and_then(|s| DateTime::parse_from_rfc3339(s).ok());
                let happened = pr.happened_at(kind).and_then(|at| DateTime::parse_from_rfc3339(at).ok());
                matches!((since, happened), (Some(since), Some(happened)) if happened >= since)
                    && !self.started.contains(&pr.url)
            })
            .collect();
        new.sort_by(|a, b| a.happened_at(kind).cmp(&b.happened_at(kind)).then(a.url.cmp(&b.url)));
        new.dedup_by(|a, b| a.url == b.url);
        new
    }
}

#[derive(Serialize, Deserialize, Default)]
struct State {
    #[serde(default)]
    jobs: BTreeMap<String, Watch>,
}

impl State {
    fn load() -> Self {
        fs::read_to_string(paths::events_file())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let Ok(text) = serde_json::to_string_pretty(self) else { return };
        let _ = fs::create_dir_all(paths::base());
        let tmp = paths::events_file().with_extension(format!("{}.tmp", std::process::id()));
        if fs::write(&tmp, text).is_ok() {
            let _ = fs::rename(&tmp, paths::events_file());
        }
    }
}

/// Changes `events.json` under a lock, since the window and the agent both write it.
fn update(change: impl FnOnce(&mut State)) {
    let _ = fs::create_dir_all(paths::base());
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths::events_file().with_extension("lock"));
    // Released when the file closes.
    if let Ok(file) = &lock {
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    }
    let mut state = State::load();
    change(&mut state);
    state.save();
}

fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Watches for `job`'s event from now on, when it is saved or resumed, so a pull request opened before the agent's
/// first check still starts it. A paused job, or one with a schedule, forgets what it had seen.
pub fn watch(job: &Job) {
    match &job.event {
        Some(event) if job.enabled => {
            let now = stamp(Utc::now());
            update(|state| state.jobs.entry(job.id.clone()).or_default().follow(event, &now));
        }
        _ => forget(&job.id),
    }
}

pub fn forget(job_id: &str) {
    update(|state| {
        state.jobs.remove(job_id);
    });
}

/// How watching for `job`'s event goes; none for a job with a schedule.
pub fn status(job: &Job) -> Option<WatchStatus> {
    job.event.as_ref()?;
    let watch = State::load().jobs.remove(&job.id).unwrap_or_default();
    Some(WatchStatus { checked_at: watch.checked_at, problem: watch.problem, pending: watch.pending })
}

/// What one check found for a job.
struct Check {
    event: Event,
    started: Option<String>,
    problem: Option<String>,
    pending: u32,
}

/// What the agent calls every 5 minutes: starts each enabled job an event starts for the oldest pull request opened since
/// it last did. Exit code 1 when a job could not be checked.
pub fn poll() -> i32 {
    let all = match jobs::load() {
        Ok(all) => all,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let known = State::load();
    let mut checks: BTreeMap<String, Check> = BTreeMap::new();
    for job in all.iter().filter(|j| j.enabled) {
        let Some(event) = &job.event else { continue };
        let (found, problem) = list_all(job, event);
        let new = known.jobs.get(&job.id).map(|w| w.new_ones(found, event)).unwrap_or_default();
        let mut started = None;
        if let Some(first) = new.first() {
            if runs::is_running(&job.id) || runs::waiting(&job.id).is_some() {
                eprintln!("{}: {} pull request(s) {} wait for the current run", job.id, new.len(), event.kind.verb());
            } else {
                match runner::spawn(job, Trigger::PullRequest, Some(first)) {
                    Ok(()) => {
                        eprintln!("{}: started for {}", job.id, first.url);
                        started = Some(first.url.clone());
                    }
                    Err(e) => eprintln!("{}: cannot start for {}: {e}", job.id, first.url),
                }
            }
        }
        if let Some(problem) = &problem {
            eprintln!("{}: {problem}", job.id);
        }
        let pending = (new.len() - usize::from(started.is_some())) as u32;
        checks.insert(job.id.clone(), Check { event: event.clone(), started, problem, pending });
    }
    let code = i32::from(checks.values().any(|c| c.problem.is_some()));
    let now = stamp(Utc::now());
    update(|state| {
        // Paused, deleted or scheduled since: forgotten.
        state.jobs.retain(|id, _| checks.contains_key(id));
        for (id, check) in checks {
            let watch = state.jobs.entry(id).or_default();
            watch.follow(&check.event, &now);
            if let Some(url) = check.started {
                watch.started.push(url);
                let extra = watch.started.len().saturating_sub(KEEP_STARTED);
                watch.started.drain(..extra);
            }
            watch.checked_at = Some(now.clone());
            watch.problem = check.problem;
            watch.pending = check.pending;
        }
    });
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(repo: &str, number: u32, created_at: &str) -> PullRequest {
        PullRequest {
            repo: repo.into(),
            number,
            title: format!("PR {number}"),
            url: format!("https://github.com/{repo}/pull/{number}"),
            author: "me".into(),
            head: "feature".into(),
            base: "main".into(),
            draft: false,
            created_at: created_at.into(),
            merged_at: None,
        }
    }

    fn trigger(kind: EventKind, repos: &[&str], mine: bool) -> Event {
        Event { kind, repos: repos.iter().map(|r| r.to_string()).collect(), mine }
    }

    fn pull_requests(repos: &[&str], mine: bool) -> Event {
        trigger(EventKind::PullRequestOpened, repos, mine)
    }

    fn merges(repos: &[&str], mine: bool) -> Event {
        trigger(EventKind::PullRequestMerged, repos, mine)
    }

    #[test]
    fn a_repository_can_be_pasted_as_a_link() {
        assert_eq!(repo_name(" XavierClavel/claude-jobs "), "XavierClavel/claude-jobs");
        assert_eq!(repo_name("https://github.com/XavierClavel/claude-jobs"), "XavierClavel/claude-jobs");
        assert_eq!(
            repo_name("https://github.com/XavierClavel/claude-jobs/pulls?q=is%3Aopen"),
            "XavierClavel/claude-jobs"
        );
        assert_eq!(repo_name("git@github.com:XavierClavel/claude-jobs.git"), "XavierClavel/claude-jobs");
        assert_eq!(repo_name("XavierClavel/claude-jobs/"), "XavierClavel/claude-jobs");
        assert!(valid_repo("picta/backend-picta-api") && valid_repo("a/b.c_d"));
        for wrong in ["claude-jobs", "a/b/c", "/b", "a/", "a b/c", "a/..", "-_/x y"] {
            assert!(!valid_repo(wrong), "{wrong}");
        }
    }

    #[test]
    fn validate_tidies_the_repositories_and_says_what_is_wrong() {
        // Whether the GitHub CLI is there depends on the Mac: only the repositories are looked at here.
        let errors = |event: Event| match validate(event, "/usr/bin:/bin") {
            Ok(_) => vec![],
            Err(errors) => errors.into_iter().filter(|e| e != GH_MISSING).collect(),
        };
        let given = pull_requests(&["https://github.com/o/a", "o/A", "", " o/b "], true);
        if let Ok(event) = validate(given.clone(), "/usr/bin:/bin") {
            assert_eq!(event, pull_requests(&["o/a", "o/b"], true));
        }
        assert!(errors(given).is_empty());
        assert_eq!(errors(pull_requests(&[" "], true)), ["Add the repositories to watch, as owner/name."]);
        assert_eq!(
            errors(pull_requests(&["claude-jobs"], false)),
            ["claude-jobs is not a repository: write it as owner/name, such as XavierClavel/claude-jobs."]
        );
    }

    #[test]
    fn events_keep_their_json_shape() {
        let value = serde_json::to_value(pull_requests(&["o/a"], true)).expect("serializes");
        assert_eq!(value, serde_json::json!({"kind": "pull_request_opened", "repos": ["o/a"], "mine": true}));
        let value = serde_json::to_value(merges(&["o/a"], false)).expect("serializes");
        assert_eq!(value, serde_json::json!({"kind": "pull_request_merged", "repos": ["o/a"], "mine": false}));
        // A job saved from the command line watches your pull requests unless it says otherwise.
        let given: Event =
            serde_json::from_str(r#"{"kind": "pull_request_opened", "repos": ["o/a"]}"#).expect("parses");
        assert_eq!(given, pull_requests(&["o/a"], true));
    }

    #[test]
    fn the_github_clis_list_is_read() {
        let json = r#"[{"author":{"id":"U_1","is_bot":false,"login":"XavierClavel","name":"Xavier"},
            "baseRefName":"main","createdAt":"2026-11-03T11:37:24Z","headRefName":"bump","isDraft":true,
            "number":20,"title":"chore: release","url":"https://github.com/o/a/pull/20"}]"#;
        let found = parse_list("o/a", json).expect("parses");
        assert_eq!(
            found,
            vec![PullRequest {
                title: "chore: release".into(),
                author: "XavierClavel".into(),
                head: "bump".into(),
                draft: true,
                ..pr("o/a", 20, "2026-11-03T11:37:24Z")
            }]
        );
        assert!(parse_list("o/a", "not json").unwrap_err().starts_with("o/a: the GitHub CLI's answer"));
    }

    #[test]
    fn the_github_clis_errors_say_what_to_do() {
        let signed_out = "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.";
        assert_eq!(gh_problem("o/a", signed_out), GH_SIGNED_OUT);
        assert_eq!(
            gh_problem("o/nope", "GraphQL: Could not resolve to a Repository with the name 'o/nope'. (repository)\n"),
            "o/nope: no such repository, or your GitHub account cannot see it."
        );
        assert_eq!(
            gh_problem(
                "o/a",
                "error connecting to api.github.com\ncheck your internet connection or https://githubstatus.com\n"
            ),
            "Cannot reach GitHub: check your internet connection or https://githubstatus.com"
        );
        let expired = "non-200 OK status code: 401 Unauthorized body: \"{\\r\\n  \\\"message\\\": \\\"Requires authentication\\\"}\"";
        assert_eq!(gh_problem("o/a", expired), "The GitHub CLI's login no longer works: run gh auth login.");
        assert_eq!(gh_problem("o/a", "something odd\n\n"), "o/a: something odd");
    }

    #[test]
    fn a_pull_request_opened_since_the_job_began_watching_is_new_once() {
        let mut watch = Watch::default();
        watch.follow(&pull_requests(&["o/a", "o/b"], true), "2026-11-03T10:00:00Z");
        let found = vec![
            pr("o/a", 3, "2026-11-03T10:05:00Z"),
            pr("o/a", 2, "2026-11-03T09:59:59Z"),
            pr("o/b", 7, "2026-11-03T10:01:00Z"),
            pr("o/c", 1, "2026-11-03T10:02:00Z"),
        ];
        // Oldest first; one opened before, or in a repository it does not watch, is not new.
        let new = watch.new_ones(found.clone(), &pull_requests(&["o/a", "o/b"], true));
        assert_eq!(new.iter().map(|p| p.number).collect::<Vec<_>>(), [7, 3]);
        watch.started.push(new[0].url.clone());
        let numbers = |new: Vec<PullRequest>| new.iter().map(|p| p.number).collect::<Vec<_>>();
        assert_eq!(numbers(watch.new_ones(found.clone(), &pull_requests(&["o/a", "o/b"], true))), [3]);
        // Watching anyone's pull requests now, or merges: it starts over from then.
        for other in [pull_requests(&["o/a", "o/b"], false), merges(&["o/a", "o/b"], true)] {
            assert!(watch.new_ones(found.clone(), &other).is_empty());
        }
        watch.follow(&pull_requests(&["o/a", "o/b"], false), "2026-11-03T11:00:00Z");
        assert!(watch.new_ones(found, &pull_requests(&["o/a", "o/b"], false)).is_empty());
        assert!(watch.started.is_empty());
    }

    #[test]
    fn a_pull_request_merged_since_the_job_began_watching_is_new_once() {
        let event = merges(&["o/a"], false);
        let mut watch = Watch::default();
        watch.follow(&event, "2026-11-03T10:00:00Z");
        let merged = |number, opened: &str, merged: &str| PullRequest {
            merged_at: Some(merged.into()),
            ..pr("o/a", number, opened)
        };
        let found = vec![
            // Opened long before, merged since: new.
            merged(1, "2026-10-01T00:00:00Z", "2026-11-03T12:00:00Z"),
            merged(2, "2026-11-03T10:30:00Z", "2026-11-03T11:00:00Z"),
            merged(3, "2026-10-01T00:00:00Z", "2026-11-03T09:00:00Z"),
        ];
        let new = watch.new_ones(found.clone(), &event);
        assert_eq!(new.iter().map(|p| p.number).collect::<Vec<_>>(), [2, 1]);
        watch.started.push(new[0].url.clone());
        assert_eq!(watch.new_ones(found, &event).iter().map(|p| p.number).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn a_repository_added_later_is_watched_from_then() {
        let mut watch = Watch::default();
        watch.follow(&pull_requests(&["o/a"], true), "2026-11-01T00:00:00Z");
        watch.follow(&pull_requests(&["o/a", "o/b"], true), "2026-11-03T10:00:00Z");
        assert_eq!(watch.since["o/a"], "2026-11-01T00:00:00Z");
        assert_eq!(watch.since["o/b"], "2026-11-03T10:00:00Z");
        let found = vec![pr("o/a", 1, "2026-11-02T00:00:00Z"), pr("o/b", 1, "2026-11-02T00:00:00Z")];
        let new = watch.new_ones(found, &pull_requests(&["o/a", "o/b"], true));
        assert_eq!(new.iter().map(|p| p.repo.as_str()).collect::<Vec<_>>(), ["o/a"]);
        watch.follow(&pull_requests(&["o/b"], true), "2026-11-04T00:00:00Z");
        assert!(!watch.since.contains_key("o/a"));
    }

    #[test]
    fn the_repositories_are_listed_newest_push_first() {
        let lines = r#"{"name":"o/old","private":false,"pushed_at":"2025-01-01T00:00:00Z"}
{"name":"o/new","private":true,"pushed_at":"2026-11-04T08:59:37Z","owner_avatar":"https://avatars.githubusercontent.com/u/1?v=4"}

{"name":"o/empty","private":false,"pushed_at":null}
"#;
        let repos = parse_repos(lines).expect("parses");
        assert_eq!(repos.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["o/new", "o/old", "o/empty"]);
        assert!(repos[0].private);
        assert_eq!(repos[0].owner_avatar.as_deref(), Some("https://avatars.githubusercontent.com/u/1?v=4"));
        assert_eq!(repos[1].owner_avatar, None);
        assert!(parse_repos("[]").is_err());
    }

    #[test]
    fn the_run_gets_the_pull_request_after_the_prompt() {
        let pr = PullRequest { title: "feat: triggers".into(), author: "XavierClavel".into(), ..pr("o/a", 21, "") };
        assert_eq!(
            prompt("/quality-gate", &pr),
            "/quality-gate\n\nPull request o/a#21 opened: feat: triggers\nhttps://github.com/o/a/pull/21\nBy \
             XavierClavel, from feature into main."
        );
        assert!(prompt("Review it.", &PullRequest { draft: true, ..pr.clone() }).ends_with("into main, as a draft."));
        let merged = PullRequest { merged_at: Some("2026-11-03T12:00:00Z".into()), ..pr };
        assert_eq!(
            prompt("/changelog", &merged),
            "/changelog\n\nPull request o/a#21 merged: feat: triggers\nhttps://github.com/o/a/pull/21\nBy \
             XavierClavel, from feature into main, merged at 2026-11-03T12:00:00Z."
        );
    }

    #[test]
    fn the_state_keeps_its_json_shape() {
        let mut state = State::default();
        state.jobs.insert("review".into(), Watch { mine: Some(true), pending: 2, ..Watch::default() });
        let text = serde_json::to_string(&state).expect("serializes");
        let back: State = serde_json::from_str(&text).expect("parses");
        assert_eq!(back.jobs["review"], state.jobs["review"]);
        assert!(serde_json::from_str::<State>("{}").expect("parses").jobs.is_empty());
    }
}
