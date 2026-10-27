//! When a job runs next, in local time, as launchd computes it from `StartCalendarInterval`.

use crate::jobs::Job;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, TimeDelta, TimeZone};

/// A catch-up more than this late is skipped: the next scheduled run is closer than the missed one.
const CATCH_UP_WINDOW: TimeDelta = TimeDelta::hours(24);

fn slot(job: &Job, day: NaiveDate) -> Option<DateTime<Local>> {
    if !job.weekdays.is_empty() && !job.weekdays.contains(&(day.weekday().number_from_monday() as u8)) {
        return None;
    }
    let at = day.and_hms_opt(job.hour.into(), job.minute.into(), 0)?;
    // On the night clocks go forward, a time that does not exist moves to the next valid one.
    Local.from_local_datetime(&at).earliest()
}

pub fn next_run(job: &Job, now: DateTime<Local>) -> Option<DateTime<Local>> {
    if !job.enabled {
        return None;
    }
    (0..8).filter_map(|o| slot(job, now.date_naive().checked_add_days(Days::new(o))?)).find(|t| *t > now)
}

/// The last scheduled time at or before `now`.
pub fn previous_run(job: &Job, now: DateTime<Local>) -> Option<DateTime<Local>> {
    (0..8).filter_map(|o| slot(job, now.date_naive().checked_sub_days(Days::new(o))?)).find(|t| *t <= now)
}

/// The scheduled time to catch up at login, if the job wants it and nothing ran since that time.
pub fn missed_run<Tz: TimeZone>(
    job: &Job,
    now: DateTime<Local>,
    last_started: Option<DateTime<Tz>>,
) -> Option<DateTime<Local>> {
    if !job.enabled || !job.catch_up {
        return None;
    }
    let slot = previous_run(job, now)?;
    let ran_since = last_started.is_some_and(|t| t >= slot);
    (now - slot <= CATCH_UP_WINDOW && !ran_since).then_some(slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(hour: u8, minute: u8, weekdays: Vec<u8>) -> Job {
        Job {
            id: "t".into(),
            name: "t".into(),
            prompt: "p".into(),
            hour,
            minute,
            weekdays,
            cwd: String::new(),
            allowed_tools: vec![],
            model: String::new(),
            timeout_minutes: 30,
            enabled: true,
            claude: String::new(),
            path: String::new(),
            interactive: false,
            catch_up: false,
        }
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).earliest().expect("valid local time")
    }

    #[test]
    fn daily_job_runs_later_today_or_tomorrow() {
        // 2026-10-27 is a Tuesday.
        assert_eq!(next_run(&job(9, 0, vec![]), at(2026, 10, 27, 8, 0)), Some(at(2026, 10, 27, 9, 0)));
        assert_eq!(next_run(&job(9, 0, vec![]), at(2026, 10, 27, 9, 0)), Some(at(2026, 10, 28, 9, 0)));
    }

    #[test]
    fn weekday_job_skips_other_days() {
        // Only Mondays: from Tuesday 27 the next is Monday 2 November.
        assert_eq!(next_run(&job(9, 0, vec![1]), at(2026, 10, 27, 8, 0)), Some(at(2026, 11, 2, 9, 0)));
    }

    #[test]
    fn previous_run_is_today_once_the_time_has_passed() {
        assert_eq!(previous_run(&job(9, 0, vec![]), at(2026, 10, 27, 9, 16)), Some(at(2026, 10, 27, 9, 0)));
        assert_eq!(previous_run(&job(9, 0, vec![]), at(2026, 10, 27, 8, 0)), Some(at(2026, 10, 26, 9, 0)));
        // Only Mondays: on Tuesday 27 the last one was Monday 26.
        assert_eq!(previous_run(&job(9, 0, vec![1]), at(2026, 10, 27, 8, 0)), Some(at(2026, 10, 26, 9, 0)));
    }

    fn catching_up(hour: u8) -> Job {
        Job { catch_up: true, ..job(hour, 0, vec![]) }
    }

    #[test]
    fn a_time_the_mac_was_off_for_is_caught_up() {
        // Booted at 09:16, last run the day before.
        let missed = missed_run(&catching_up(9), at(2026, 10, 27, 9, 16), Some(at(2026, 10, 26, 16, 1)));
        assert_eq!(missed, Some(at(2026, 10, 27, 9, 0)));
        assert_eq!(missed_run(&catching_up(9), at(2026, 10, 27, 9, 16), None::<DateTime<Local>>), missed);
    }

    #[test]
    fn nothing_is_caught_up_after_a_run_or_too_late() {
        let now = at(2026, 10, 27, 9, 16);
        assert_eq!(missed_run(&catching_up(9), now, Some(at(2026, 10, 27, 9, 0))), None);
        // Logged in before 09:00: yesterday's run happened.
        assert_eq!(missed_run(&catching_up(9), at(2026, 10, 27, 8, 0), Some(at(2026, 10, 26, 9, 0))), None);
        // Only Mondays, logged in on Wednesday: two days late.
        let monday = Job { catch_up: true, ..job(9, 0, vec![1]) };
        assert_eq!(missed_run(&monday, at(2026, 10, 28, 10, 0), Some(at(2026, 10, 19, 9, 0))), None);
    }

    #[test]
    fn catch_up_is_opt_in() {
        assert_eq!(missed_run(&job(9, 0, vec![]), at(2026, 10, 27, 9, 16), None::<DateTime<Local>>), None);
    }

    #[test]
    fn paused_job_has_no_next_run() {
        let mut paused = job(9, 0, vec![]);
        paused.enabled = false;
        assert_eq!(next_run(&paused, at(2026, 10, 27, 8, 0)), None);
    }
}
