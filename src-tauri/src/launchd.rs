//! Each enabled job on a schedule is a launchd agent, `com.claude-jobs.<id>`, that calls this app's binary with
//! `run <id>`. One more agent, `com.claude-jobs.app.catch-up`, calls `catch-up` at login while such a job is enabled,
//! and `com.claude-jobs.app.events` calls `poll` every 5 minutes while a job an event starts is.

use crate::cron::Cron;
use crate::jobs::Job;
use crate::paths;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LABEL_PREFIX: &str = "com.claude-jobs.";
/// Not a valid job id, so it cannot clash with a job's agent.
const CATCH_UP: &str = "app.catch-up";
const EVENTS: &str = "app.events";
/// How often the events agent checks, in seconds.
const EVENTS_INTERVAL: u32 = 300;

pub fn label(job_id: &str) -> String {
    format!("{LABEL_PREFIX}{job_id}")
}

pub fn plist_path(job_id: &str) -> PathBuf {
    paths::agents_dir().join(format!("{}.plist", label(job_id)))
}

fn domain() -> String {
    format!("gui/{}", unsafe { libc::getuid() })
}

fn launchctl(args: &[&str]) -> Option<Output> {
    Command::new("launchctl").args(args).output().ok()
}

pub fn is_loaded(job_id: &str) -> bool {
    launchctl(&["print", &format!("{}/{}", domain(), label(job_id))]).is_some_and(|o| o.status.success())
}

/// A key left out matches any value.
#[derive(Serialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "PascalCase")]
struct CalendarEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    month: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    day: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    weekday: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hour: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    minute: Option<u8>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(untagged)]
enum CalendarInterval {
    Daily(CalendarEntry),
    Days(Vec<CalendarEntry>),
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Agent {
    label: String,
    program_arguments: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_calendar_interval: Option<CalendarInterval>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_interval: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    run_at_load: bool,
    environment_variables: BTreeMap<String, String>,
    standard_out_path: String,
    standard_error_path: String,
    process_type: String,
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

/// The most entries a cron expression gets. Past it, the field with the most values matches any, until it fits: launchd
/// then starts the job more often than the expression says, and the runner skips the starts that are not due.
const MAX_CRON_ENTRIES: usize = 1000;

/// Each value, or a single None for every value: launchd matches a key left out with any.
fn values(all: &[u8], every: usize) -> Vec<Option<u8>> {
    if all.len() == every {
        vec![None]
    } else {
        all.iter().map(|&v| Some(v)).collect()
    }
}

/// launchd starts a job when the minute, hour and month of one of its entries match, and its day of the month or its
/// day of the week. So each cron entry holds one of them at most.
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

fn interval(job: &Job) -> CalendarInterval {
    // jobs.json edited by hand can hold an expression that does not read: the job then keeps its time and days.
    if let Some(Ok(cron)) = job.cron.as_deref().map(Cron::parse) {
        return CalendarInterval::Days(cron_entries(&cron));
    }
    let at =
        |month, day, weekday| CalendarEntry { month, day, weekday, hour: Some(job.hour), minute: Some(job.minute) };
    if !job.month_days.is_empty() {
        let mut entries: Vec<CalendarEntry> = job.month_days.iter().map(|&d| at(None, Some(d), None)).collect();
        if job.last_day_fallback {
            // launchd has no year: 28 February starts the job in leap years too, and the runner skips that start.
            entries.extend(
                MONTH_ENDS
                    .iter()
                    .filter(|(_, end)| !job.month_days.contains(end) && job.month_days.iter().any(|d| d > end))
                    .map(|&(month, end)| at(Some(month), Some(end), None)),
            );
        }
        return CalendarInterval::Days(entries);
    }
    if job.weekdays.is_empty() {
        return CalendarInterval::Daily(at(None, None, None));
    }
    // launchd counts Sunday as 0, the editor as 7.
    CalendarInterval::Days(job.weekdays.iter().map(|d| at(None, None, Some(d % 7))).collect())
}

fn write_plist(id: &str, agent: &Agent) -> Result<(), String> {
    fs::create_dir_all(paths::agents_dir()).map_err(|e| e.to_string())?;
    let path = plist_path(id);
    plist::to_file_xml(&path, agent).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let lint = Command::new("plutil").arg("-lint").arg(&path).output().map_err(|e| e.to_string())?;
    if !lint.status.success() {
        return Err(format!("the launchd file is invalid: {}", String::from_utf8_lossy(&lint.stdout).trim()));
    }
    Ok(())
}

fn log_path(name: &str) -> Result<String, String> {
    fs::create_dir_all(paths::logs_dir()).map_err(|e| e.to_string())?;
    Ok(paths::logs_dir().join(format!("{name}.launchd.log")).display().to_string())
}

fn job_agent(job: &Job, exe: &Path) -> Result<Agent, String> {
    let log = log_path(&job.id)?;
    Ok(Agent {
        label: label(&job.id),
        program_arguments: vec![exe.display().to_string(), "run".into(), job.id.clone()],
        start_calendar_interval: Some(interval(job)),
        start_interval: None,
        run_at_load: false,
        environment_variables: BTreeMap::from([
            ("PATH".to_string(), job.path.clone()),
            ("HOME".to_string(), paths::home().display().to_string()),
        ]),
        standard_out_path: log.clone(),
        standard_error_path: log,
        process_type: "Background".into(),
    })
}

fn load(id: &str, agent: &Agent) -> Result<(), String> {
    let _ = launchctl(&["bootout", &format!("{}/{}", domain(), label(id))]);
    write_plist(id, agent)?;
    let path = plist_path(id).display().to_string();
    let out = launchctl(&["bootstrap", &domain(), &path]).ok_or("cannot run launchctl")?;
    if !out.status.success() && !is_loaded(id) {
        return Err(format!("launchctl bootstrap failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

/// The agent that starts `job`: its own, or the events agent for a job an event starts.
pub fn agent_of(job: &Job) -> &str {
    if job.event.is_some() {
        EVENTS
    } else {
        &job.id
    }
}

/// Makes launchd match the job: loaded with its schedule when enabled, gone when paused or started by an event.
pub fn apply(job: &Job, exe: &Path) -> Result<(), String> {
    if !job.enabled || job.event.is_some() {
        remove(&job.id);
        return Ok(());
    }
    load(&job.id, &job_agent(job, exe)?)
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
    if program_of(CATCH_UP).as_deref() == Some(&*exe.display().to_string()) && is_loaded(CATCH_UP) {
        return Ok(());
    }
    let log = log_path(CATCH_UP)?;
    let agent = Agent {
        label: label(CATCH_UP),
        program_arguments: vec![exe.display().to_string(), "catch-up".into()],
        start_calendar_interval: None,
        start_interval: None,
        run_at_load: true,
        environment_variables: BTreeMap::from([
            ("PATH".to_string(), "/usr/bin:/bin:/usr/sbin:/sbin".to_string()),
            ("HOME".to_string(), paths::home().display().to_string()),
        ]),
        standard_out_path: log.clone(),
        standard_error_path: log,
        process_type: "Background".into(),
    };
    load(CATCH_UP, &agent)
}

/// Loads the events agent while a job an event starts is enabled, and removes it otherwise. Loading it runs it once.
fn apply_events(jobs: &[Job], exe: &Path) -> Result<(), String> {
    if !jobs.iter().any(|j| j.enabled && j.event.is_some()) {
        remove(EVENTS);
        return Ok(());
    }
    if program_of(EVENTS).as_deref() == Some(&*exe.display().to_string())
        && interval_of(EVENTS) == Some(EVENTS_INTERVAL)
        && is_loaded(EVENTS)
    {
        return Ok(());
    }
    load(EVENTS, &events_agent(exe)?)
}

fn events_agent(exe: &Path) -> Result<Agent, String> {
    let log = log_path(EVENTS)?;
    Ok(Agent {
        label: label(EVENTS),
        program_arguments: vec![exe.display().to_string(), "poll".into()],
        start_calendar_interval: None,
        start_interval: Some(EVENTS_INTERVAL),
        run_at_load: true,
        // The GitHub CLI and the runs get each job's own PATH.
        environment_variables: BTreeMap::from([
            ("PATH".to_string(), "/usr/bin:/bin:/usr/sbin:/sbin".to_string()),
            ("HOME".to_string(), paths::home().display().to_string()),
        ]),
        standard_out_path: log.clone(),
        standard_error_path: log,
        process_type: "Background".into(),
    })
}

pub fn remove(job_id: &str) {
    let _ = launchctl(&["bootout", &format!("{}/{}", domain(), label(job_id))]);
    let _ = fs::remove_file(plist_path(job_id));
}

/// The binary a job's launchd file calls, to repoint it when the app moves or replaces the Python runner.
pub fn program_of(job_id: &str) -> Option<String> {
    let value = plist::Value::from_file(plist_path(job_id)).ok()?;
    let args = value.as_dictionary()?.get("ProgramArguments")?.as_array()?;
    args.first()?.as_string().map(str::to_string)
}

/// How often, in seconds, an agent's launchd file starts it, for one started at an interval.
fn interval_of(id: &str) -> Option<u32> {
    let value = plist::Value::from_file(plist_path(id)).ok()?;
    value.as_dictionary()?.get("StartInterval")?.as_unsigned_integer().and_then(|n| u32::try_from(n).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(weekdays: Vec<u8>) -> Job {
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

    #[test]
    fn every_day_is_one_entry_without_a_weekday() {
        assert_eq!(
            interval(&job(vec![])),
            CalendarInterval::Daily(CalendarEntry {
                month: None,
                day: None,
                weekday: None,
                hour: Some(9),
                minute: Some(30)
            })
        );
    }

    fn cron(expression: &str) -> Vec<CalendarEntry> {
        let CalendarInterval::Days(entries) = interval(&Job { cron: Some(expression.into()), ..job(vec![]) }) else {
            panic!("expected entries")
        };
        entries
    }

    fn entry(day: Option<u8>, weekday: Option<u8>, hour: Option<u8>, minute: Option<u8>) -> CalendarEntry {
        CalendarEntry { month: None, day, weekday, hour, minute }
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
        // Odd days that are Mondays: launchd starts it on odd days, and the runner skips the others.
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
        let mut xml = Vec::new();
        plist::to_writer_xml(&mut xml, &CalendarInterval::Days(busy)).expect("serializes");
        assert!(!String::from_utf8(xml).expect("UTF-8").contains("Minute"));
    }

    #[test]
    fn a_job_on_the_31st_also_starts_at_the_end_of_shorter_months() {
        let month_end = Job { month_days: vec![31], last_day_fallback: true, ..job(vec![]) };
        let CalendarInterval::Days(entries) = interval(&month_end) else { panic!("expected days") };
        let dates: Vec<_> = entries.iter().map(|e| (e.month, e.day)).collect();
        let shorter = [(2, 28), (2, 29), (4, 30), (6, 30), (9, 30), (11, 30)].map(|(m, d)| (Some(m), Some(d)));
        assert_eq!(dates, [vec![(None, Some(31))], shorter.to_vec()].concat());
    }

    #[test]
    fn the_shared_agents_cannot_clash_with_a_job() {
        assert!(!crate::jobs::valid_id(CATCH_UP));
        assert!(!crate::jobs::valid_id(EVENTS));
    }

    #[test]
    fn the_events_agent_checks_every_minute_from_load() {
        let agent = events_agent(Path::new("/Applications/Claude Jobs.app/Contents/MacOS/claude-jobs")).expect("agent");
        let mut xml = Vec::new();
        plist::to_writer_xml(&mut xml, &agent).expect("serializes");
        let xml = String::from_utf8(xml).expect("UTF-8");
        assert!(xml.contains("<key>StartInterval</key>\n\t<integer>300</integer>"), "{xml}");
        assert!(xml.contains("<key>RunAtLoad</key>") && xml.contains("<string>poll</string>"), "{xml}");
        assert!(!xml.contains("StartCalendarInterval"));
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
    fn sunday_is_zero_for_launchd() {
        let CalendarInterval::Days(days) = interval(&job(vec![1, 7])) else { panic!("expected days") };
        assert_eq!(days.iter().map(|d| d.weekday).collect::<Vec<_>>(), vec![Some(1), Some(0)]);
    }

    #[test]
    fn a_monthly_job_has_one_day_entry_per_day_of_the_month() {
        let monthly = Job { month_days: vec![1, 15], ..job(vec![]) };
        let mut xml = Vec::new();
        plist::to_writer_xml(&mut xml, &interval(&monthly)).expect("serializes");
        let xml = String::from_utf8(xml).expect("UTF-8");
        assert_eq!(xml.matches("<key>Day</key>").count(), 2);
        assert!(!xml.contains("Weekday"));
    }
}
