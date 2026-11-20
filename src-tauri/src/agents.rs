//! Each enabled job on a schedule is an agent, `com.claude-jobs.<id>`, that calls this app's binary with `run <id>`:
//! a launchd agent on macOS (see `launchd`), a systemd user timer on Linux (see `systemd`). One more agent,
//! `com.claude-jobs.app.catch-up`, calls `catch-up` at login while such a job is enabled, and
//! `com.claude-jobs.app.events` calls `poll` every 5 minutes while a job an event starts is.

use crate::cron::Cron;
use crate::jobs::{self, Job};
use crate::paths;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[cfg(target_os = "macos")]
use crate::launchd as backend;
#[cfg(target_os = "linux")]
use crate::systemd as backend;

pub use backend::{is_loaded, remove};

const LABEL_PREFIX: &str = "com.claude-jobs.";
/// Not a valid job id, so it cannot clash with a job's agent.
const CATCH_UP: &str = "app.catch-up";
const EVENTS: &str = "app.events";
/// How often the events agent checks, in seconds.
const EVENTS_INTERVAL: u32 = 300;
/// The shared agents' PATH: the GitHub CLI and the runs get each job's own.
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

pub fn label(id: &str) -> String {
    format!("{LABEL_PREFIX}{id}")
}

/// A time an agent starts at. A key left out matches any value.
#[derive(Serialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "PascalCase")]
pub struct CalendarEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<u8>,
    /// 0 (Sunday) to 6.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekday: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hour: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minute: Option<u8>,
}

/// When an agent starts its program.
#[derive(Debug, PartialEq)]
pub enum Start {
    /// At each of these times.
    Calendar(Vec<CalendarEntry>),
    /// At login, and once when it is loaded.
    Login,
    /// Every this many seconds, from when it is loaded on.
    Every(u32),
}

/// What calls the app's binary for a job, or for all of them: with which arguments and environment, when, and where
/// what it prints goes.
#[derive(Debug)]
pub struct Agent {
    pub id: String,
    /// The binary, then its arguments.
    pub args: Vec<String>,
    pub start: Start,
    pub env: BTreeMap<String, String>,
    pub log: String,
}

/// The last day of each month, February twice for leap years.
const MONTH_ENDS: [(u8, u8); 13] = [
    (1, 31),
    (2, 28),
    (2, 29),
    (3, 31),
    (4, 30),
    (5, 31),
    (6, 30),
    (7, 31),
    (8, 31),
    (9, 30),
    (10, 31),
    (11, 30),
    (12, 31),
];

/// The most entries a cron expression gets. Past it, the field with the most values matches any, until it fits: the
/// agent then starts the job more often than the expression says, and the runner skips the starts that are not due.
const MAX_CRON_ENTRIES: usize = 1000;

/// Each value, or a single None for every value: a key left out matches any.
fn values(all: &[u8], every: usize) -> Vec<Option<u8>> {
    if all.len() == every {
        vec![None]
    } else {
        all.iter().map(|&v| Some(v)).collect()
    }
}

/// launchd starts a job when the minute, hour and month of one of its entries match, and its day of the month or its
/// day of the week; systemd when both days do. So each cron entry holds one of them at most, which both read alike.
fn cron_entries(cron: &Cron) -> Vec<CalendarEntry> {
    let (mut minutes, mut hours, mut months) =
        (values(&cron.minutes, 60), values(&cron.hours, 24), values(&cron.months, 12));
    let (days, weekdays) = (values(&cron.days, 31), values(&cron.weekdays, 7));
    let by_date = days.iter().map(|&d| (d, None));
    let by_weekday = weekdays.iter().map(|&w| (None, w));
    let mut dates: Vec<(Option<u8>, Option<u8>)> = if cron.both_days && days == [None] {
        by_weekday.collect()
    } else if cron.both_days {
        // Both have to match, which launchd cannot say: the days of the month alone, which the runner narrows.
        by_date.collect()
    } else {
        by_date.chain(by_weekday).collect()
    };
    if dates.contains(&(None, None)) {
        dates = vec![(None, None)];
    }
    while dates.len() * months.len() * hours.len() * minutes.len() > MAX_CRON_ENTRIES {
        let widest = [dates.len(), months.len(), hours.len(), minutes.len()];
        match widest.iter().enumerate().max_by_key(|(_, n)| **n).map(|(i, _)| i) {
            Some(0) => dates = vec![(None, None)],
            Some(1) => months = vec![None],
            Some(2) => hours = vec![None],
            _ => minutes = vec![None],
        }
    }
    // An entry without a key would be every minute; spelt out, so launchd cannot read it as nothing.
    if dates == [(None, None)] && months == [None] && hours == [None] && minutes == [None] {
        minutes = (0..60).map(Some).collect();
    }
    let mut entries = Vec::new();
    for &(day, weekday) in &dates {
        for &month in &months {
            for &hour in &hours {
                for &minute in &minutes {
                    entries.push(CalendarEntry { month, day, weekday, hour, minute });
                }
            }
        }
    }
    entries
}

/// The times a job on a schedule starts at.
pub fn calendar(job: &Job) -> Vec<CalendarEntry> {
    // jobs.json edited by hand can hold an expression that does not read: the job then keeps its time and days.
    if let Some(Ok(cron)) = job.cron.as_deref().map(Cron::parse) {
        return cron_entries(&cron);
    }
    let at =
        |month, day, weekday| CalendarEntry { month, day, weekday, hour: Some(job.hour), minute: Some(job.minute) };
    if !job.month_days.is_empty() {
        let mut entries: Vec<CalendarEntry> = job.month_days.iter().map(|&d| at(None, Some(d), None)).collect();
        if job.last_day_fallback {
            // No year in an entry: 28 February starts the job in leap years too, and the runner skips that start.
            entries.extend(
                MONTH_ENDS
                    .iter()
                    .filter(|(_, end)| !job.month_days.contains(end) && job.month_days.iter().any(|d| d > end))
                    .map(|&(month, end)| at(Some(month), Some(end), None)),
            );
        }
        return entries;
    }
    if job.weekdays.is_empty() {
        return vec![at(None, None, None)];
    }
    // An entry counts Sunday as 0, the editor as 7.
    job.weekdays.iter().map(|d| at(None, None, Some(d % 7))).collect()
}

fn agent(id: &str, args: Vec<String>, start: Start, path: &str) -> Result<Agent, String> {
    fs::create_dir_all(paths::logs_dir()).map_err(|e| e.to_string())?;
    Ok(Agent {
        id: id.to_string(),
        args,
        start,
        env: BTreeMap::from([
            ("PATH".to_string(), path.to_string()),
            ("HOME".to_string(), paths::home().display().to_string()),
        ]),
        log: paths::agent_log(id).display().to_string(),
    })
}

fn job_agent(job: &Job, exe: &Path) -> Result<Agent, String> {
    let args = vec![exe.display().to_string(), "run".into(), job.id.clone()];
    agent(&job.id, args, Start::Calendar(calendar(job)), &job.path)
}

fn catch_up_agent(exe: &Path) -> Result<Agent, String> {
    agent(CATCH_UP, vec![exe.display().to_string(), "catch-up".into()], Start::Login, SYSTEM_PATH)
}

fn events_agent(exe: &Path) -> Result<Agent, String> {
    agent(EVENTS, vec![exe.display().to_string(), "poll".into()], Start::Every(EVENTS_INTERVAL), SYSTEM_PATH)
}

/// The agent that starts `job`: its own, or the events agent for a job an event starts.
pub fn agent_of(job: &Job) -> &str {
    if job.event.is_some() {
        EVENTS
    } else {
        &job.id
    }
}

/// Makes the agent match the job: loaded with its schedule when enabled, gone when paused or started by an event.
pub fn apply(job: &Job, exe: &Path) -> Result<(), String> {
    if !job.enabled || job.event.is_some() {
        remove(&job.id);
        return Ok(());
    }
    backend::load(&job_agent(job, exe)?)
}

/// The agents jobs share: the login catch-up, and the events check.
pub fn apply_shared(jobs: &[Job], exe: &Path) -> Result<(), String> {
    apply_catch_up(jobs, exe)?;
    apply_events(jobs, exe)
}

/// Loads the login agent while a job on a schedule is enabled, and removes it otherwise. Loading it runs it once.
fn apply_catch_up(jobs: &[Job], exe: &Path) -> Result<(), String> {
    if !jobs.iter().any(|j| j.enabled && j.event.is_none()) {
        remove(CATCH_UP);
        return Ok(());
    }
    load_unless_current(&catch_up_agent(exe)?)
}

/// Loads the events agent while a job an event starts is enabled, and removes it otherwise. Loading it runs it once.
fn apply_events(jobs: &[Job], exe: &Path) -> Result<(), String> {
    if !jobs.iter().any(|j| j.enabled && j.event.is_some()) {
        remove(EVENTS);
        return Ok(());
    }
    load_unless_current(&events_agent(exe)?)
}

fn load_unless_current(agent: &Agent) -> Result<(), String> {
    if backend::is_current(agent) {
        return Ok(());
    }
    backend::load(agent)
}

/// Points every enabled job's agent at `exe`, this binary: after an update, a move, or on the first start after the
/// Python version.
pub fn sync(exe: &Path) {
    let Ok(all) = jobs::load() else { return };
    for job in all.iter().filter(|j| j.enabled) {
        let stale = match &job.event {
            // Its own agent, left from when it had a schedule.
            Some(_) => backend::path(&job.id).exists(),
            None => job_agent(job, exe).map_or(true, |agent| !backend::is_current(&agent)),
        };
        if stale {
            if let Err(e) = apply(job, exe) {
                eprintln!("could not update the schedule of {}: {e}", job.id);
            }
        }
    }
    if let Err(e) = apply_shared(&all, exe) {
        eprintln!("could not update the login catch-up or the events check: {e}");
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn job(weekdays: Vec<u8>) -> Job {
        Job {
            id: "t".into(),
            name: "t".into(),
            prompt: "p".into(),
            hour: 9,
            minute: 30,
            weekdays,
            month_days: vec![],
            last_day_fallback: false,
            cron: None,
            cwd: String::new(),
            allowed_tools: vec![],
            model: String::new(),
            effort: None,
            timeout_minutes: 30,
            enabled: true,
            claude: String::new(),
            path: String::new(),
            interactive: false,
            chrome: false,
            conditions: vec![],
            during_run: vec![],
            on_success: vec![],
            on_failure: vec![],
            on_complete: vec![],
            event: None,
        }
    }

    pub(crate) fn entry(day: Option<u8>, weekday: Option<u8>, hour: Option<u8>, minute: Option<u8>) -> CalendarEntry {
        CalendarEntry { month: None, day, weekday, hour, minute }
    }

    pub(crate) fn cron(expression: &str) -> Vec<CalendarEntry> {
        calendar(&Job { cron: Some(expression.into()), ..job(vec![]) })
    }

    #[test]
    fn every_day_is_one_entry_without_a_weekday() {
        assert_eq!(calendar(&job(vec![])), vec![entry(None, None, Some(9), Some(30))]);
    }

    #[test]
    fn a_cron_job_has_one_entry_per_time_it_names() {
        let quarters = cron("*/15 9-17 * * 1-5");
        assert_eq!(quarters.len(), 4 * 9 * 5);
        assert_eq!(quarters[0], entry(None, Some(1), Some(9), Some(0)));
        assert_eq!(quarters[1], entry(None, Some(1), Some(9), Some(15)));
        // Every 5 minutes: any hour.
        assert_eq!(
            cron("*/5 * * * *"),
            (0..60).step_by(5).map(|m| entry(None, None, None, Some(m))).collect::<Vec<_>>()
        );
        assert_eq!(
            cron("0 9 1,15 * *"),
            vec![entry(Some(1), None, Some(9), Some(0)), entry(Some(15), None, Some(9), Some(0))]
        );
        let june = cron("30 8 * 6 *");
        assert_eq!(june, vec![CalendarEntry { month: Some(6), ..entry(None, None, Some(8), Some(30)) }]);
    }

    #[test]
    fn a_cron_job_on_the_first_or_on_mondays_has_an_entry_for_each() {
        assert_eq!(
            cron("0 9 1 * 1"),
            vec![entry(Some(1), None, Some(9), Some(0)), entry(None, Some(1), Some(9), Some(0))]
        );
        // Odd days that are Mondays: the agent starts it on odd days, and the runner skips the others.
        let odd_mondays = cron("0 9 */2 * 1");
        assert_eq!(odd_mondays.len(), 16);
        assert!(odd_mondays.iter().all(|e| e.weekday.is_none() && e.day.is_some()));
    }

    #[test]
    fn every_minute_is_spelt_out() {
        assert_eq!(cron("* * * * *"), (0..60).map(|m| entry(None, None, None, Some(m))).collect::<Vec<_>>());
    }

    #[test]
    fn a_cron_job_past_the_most_entries_starts_more_often() {
        // 30 × 9 × 5 = 1,350 entries: any minute instead, in those hours.
        let busy = cron("*/2 9-17 * * 1-5");
        assert_eq!(busy.len(), 9 * 5);
        assert!(busy.iter().all(|e| e.minute.is_none() && e.hour.is_some() && e.weekday.is_some()));
    }

    #[test]
    fn a_job_on_the_31st_also_starts_at_the_end_of_shorter_months() {
        let month_end = Job { month_days: vec![31], last_day_fallback: true, ..job(vec![]) };
        let dates: Vec<_> = calendar(&month_end).iter().map(|e| (e.month, e.day)).collect();
        let shorter = [(2, 28), (2, 29), (4, 30), (6, 30), (9, 30), (11, 30)].map(|(m, d)| (Some(m), Some(d)));
        assert_eq!(dates, [vec![(None, Some(31))], shorter.to_vec()].concat());
    }

    #[test]
    fn a_monthly_job_has_one_entry_per_day_of_the_month() {
        let monthly = Job { month_days: vec![1, 15], ..job(vec![]) };
        assert_eq!(
            calendar(&monthly),
            vec![entry(Some(1), None, Some(9), Some(30)), entry(Some(15), None, Some(9), Some(30))]
        );
    }

    #[test]
    fn the_shared_agents_cannot_clash_with_a_job() {
        assert!(!crate::jobs::valid_id(CATCH_UP));
        assert!(!crate::jobs::valid_id(EVENTS));
    }

    #[test]
    fn the_events_agent_polls_every_five_minutes_and_the_catch_up_at_login() {
        let exe = Path::new("/opt/claude-jobs");
        let events = events_agent(exe).expect("agent");
        assert_eq!((events.start, &events.args[1..]), (Start::Every(300), &["poll".to_string()][..]));
        let catch_up = catch_up_agent(exe).expect("agent");
        assert_eq!((catch_up.start, &catch_up.args[1..]), (Start::Login, &["catch-up".to_string()][..]));
    }

    #[test]
    fn a_job_an_event_starts_has_the_events_agent() {
        let watching = Job {
            event: Some(crate::events::Event {
                kind: crate::events::EventKind::PullRequestOpened,
                repos: vec!["o/a".into()],
                mine: true,
            }),
            ..job(vec![])
        };
        assert_eq!((agent_of(&job(vec![])), agent_of(&watching)), ("t", EVENTS));
    }

    #[test]
    fn sunday_is_zero() {
        let days = calendar(&job(vec![1, 7]));
        assert_eq!(days.iter().map(|d| d.weekday).collect::<Vec<_>>(), vec![Some(1), Some(0)]);
    }
}
