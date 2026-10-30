//! When a job runs next, in local time, as launchd computes it from `StartCalendarInterval`.

use crate::jobs::Job;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, TimeDelta, TimeZone};

/// A catch-up more than this late is skipped: the next scheduled run is closer than the missed one.
const CATCH_UP_WINDOW: TimeDelta = TimeDelta::hours(24);

/// How many days to look ahead or back: a job on the 31st waits 61 days from 31 March to 31 May.
const HORIZON_DAYS: u64 = 62;

fn is_last_day(day: NaiveDate) -> bool {
    day.succ_opt().map_or(true, |next| next.month() != day.month())
}

fn runs_on(job: &Job, day: NaiveDate) -> bool {
    if !job.month_days.is_empty() {
        let date = day.day() as u8;
        return job.month_days.contains(&date)
            || (job.last_day_fallback && is_last_day(day) && job.month_days.iter().any(|&d| d > date));
    }
    job.weekdays.is_empty() || job.weekdays.contains(&(day.weekday().number_from_monday() as u8))
}

fn slot(job: &Job, day: NaiveDate) -> Option<DateTime<Local>> {
    if !runs_on(job, day) {
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
    (0..=HORIZON_DAYS).filter_map(|o| slot(job, now.date_naive().checked_add_days(Days::new(o))?)).find(|t| *t > now)
}

/// The last scheduled time at or before `now`.
pub fn previous_run(job: &Job, now: DateTime<Local>) -> Option<DateTime<Local>> {
    (0..=HORIZON_DAYS).filter_map(|o| slot(job, now.date_naive().checked_sub_days(Days::new(o))?)).find(|t| *t <= now)
}

/// The scheduled time to catch up at login, if nothing ran since that time.
pub fn missed_run<Tz: TimeZone>(
    job: &Job,
    now: DateTime<Local>,
    last_started: Option<DateTime<Tz>>,
) -> Option<DateTime<Local>> {
    if !job.enabled {
        return None;
    }
    let slot = previous_run(job, now)?;
    let ran_since = last_started.is_some_and(|t| t >= slot);
    (now - slot <= CATCH_UP_WINDOW && !ran_since).then_some(slot)
}

/// Whether the last scheduled time at or before `now` already has its run, so a launchd start now is not a new one.
pub fn already_ran<Tz: TimeZone>(job: &Job, now: DateTime<Local>, last_started: Option<DateTime<Tz>>) -> bool {
    previous_run(job, now).is_some_and(|slot| last_started.is_some_and(|t| t >= slot))
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
            month_days: vec![],
            last_day_fallback: false,
            cwd: String::new(),
            allowed_tools: vec![],
            model: String::new(),
            timeout_minutes: 30,
            enabled: true,
            claude: String::new(),
            path: String::new(),
            interactive: false,
            conditions: vec![],
        }
    }

    fn monthly(month_days: Vec<u8>) -> Job {
        Job { month_days, ..job(9, 0, vec![]) }
    }

    fn month_end(month_days: Vec<u8>) -> Job {
        Job { last_day_fallback: true, ..monthly(month_days) }
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
    fn monthly_job_waits_for_its_days() {
        assert_eq!(next_run(&monthly(vec![1]), at(2026, 10, 29, 8, 0)), Some(at(2026, 11, 1, 9, 0)));
        assert_eq!(next_run(&monthly(vec![1, 15]), at(2026, 11, 1, 9, 0)), Some(at(2026, 11, 15, 9, 0)));
    }

    #[test]
    fn a_month_without_the_day_is_skipped() {
        // There is no 31 April: from 31 March, the next 31st is in May.
        assert_eq!(next_run(&monthly(vec![31]), at(2027, 3, 31, 10, 0)), Some(at(2027, 5, 31, 9, 0)));
        assert_eq!(previous_run(&monthly(vec![31]), at(2027, 5, 30, 10, 0)), Some(at(2027, 3, 31, 9, 0)));
        // February 2027 has no 29th.
        assert_eq!(next_run(&monthly(vec![29]), at(2027, 1, 30, 10, 0)), Some(at(2027, 3, 29, 9, 0)));
    }

    #[test]
    fn a_shorter_month_can_run_on_its_last_day() {
        // A job on the 31st: 30 April, then 31 May.
        assert_eq!(next_run(&month_end(vec![31]), at(2027, 3, 31, 10, 0)), Some(at(2027, 4, 30, 9, 0)));
        assert_eq!(next_run(&month_end(vec![31]), at(2027, 4, 30, 10, 0)), Some(at(2027, 5, 31, 9, 0)));
        // February: the 28th, or the 29th in a leap year, never both.
        assert_eq!(next_run(&month_end(vec![30]), at(2027, 2, 1, 10, 0)), Some(at(2027, 2, 28, 9, 0)));
        assert_eq!(next_run(&month_end(vec![30]), at(2028, 2, 1, 10, 0)), Some(at(2028, 2, 29, 9, 0)));
        // The 30th and the 31st: one run on 30 April, then 30 May.
        assert_eq!(next_run(&month_end(vec![30, 31]), at(2027, 4, 1, 10, 0)), Some(at(2027, 4, 30, 9, 0)));
        assert_eq!(next_run(&month_end(vec![30, 31]), at(2027, 4, 30, 10, 0)), Some(at(2027, 5, 30, 9, 0)));
    }

    #[test]
    fn a_start_after_the_last_run_is_not_a_new_run() {
        // launchd also starts a job on the 31st on 28 February 2028, which is not the last day of that February.
        let job = month_end(vec![31]);
        let january = Some(at(2028, 1, 31, 9, 0));
        assert!(already_ran(&job, at(2028, 2, 28, 9, 0), january));
        assert!(!already_ran(&job, at(2028, 2, 29, 9, 0), january));
        // Woken on 2 March after missing 29 February: that run is still due.
        assert!(!already_ran(&job, at(2028, 3, 2, 8, 0), january));
        assert!(!already_ran(&job, at(2028, 2, 28, 9, 0), None::<DateTime<Local>>));
    }

    #[test]
    fn previous_run_is_today_once_the_time_has_passed() {
        assert_eq!(previous_run(&job(9, 0, vec![]), at(2026, 10, 27, 9, 16)), Some(at(2026, 10, 27, 9, 0)));
        assert_eq!(previous_run(&job(9, 0, vec![]), at(2026, 10, 27, 8, 0)), Some(at(2026, 10, 26, 9, 0)));
        // Only Mondays: on Tuesday 27 the last one was Monday 26.
        assert_eq!(previous_run(&job(9, 0, vec![1]), at(2026, 10, 27, 8, 0)), Some(at(2026, 10, 26, 9, 0)));
    }

    #[test]
    fn a_time_the_mac_was_off_for_is_caught_up() {
        // Booted at 09:16, last run the day before.
        let missed = missed_run(&job(9, 0, vec![]), at(2026, 10, 27, 9, 16), Some(at(2026, 10, 26, 16, 1)));
        assert_eq!(missed, Some(at(2026, 10, 27, 9, 0)));
        assert_eq!(missed_run(&job(9, 0, vec![]), at(2026, 10, 27, 9, 16), None::<DateTime<Local>>), missed);
    }

    #[test]
    fn nothing_is_caught_up_after_a_run_or_too_late() {
        let now = at(2026, 10, 27, 9, 16);
        assert_eq!(missed_run(&job(9, 0, vec![]), now, Some(at(2026, 10, 27, 9, 0))), None);
        // Logged in before 09:00: yesterday's run happened.
        assert_eq!(missed_run(&job(9, 0, vec![]), at(2026, 10, 27, 8, 0), Some(at(2026, 10, 26, 9, 0))), None);
        // Only Mondays, logged in on Wednesday: two days late.
        assert_eq!(missed_run(&job(9, 0, vec![1]), at(2026, 10, 28, 10, 0), Some(at(2026, 10, 19, 9, 0))), None);
    }

    #[test]
    fn a_monthly_run_is_caught_up_the_day_it_was_missed() {
        let last = Some(at(2026, 10, 1, 9, 0));
        assert_eq!(missed_run(&monthly(vec![1]), at(2026, 11, 1, 9, 30), last), Some(at(2026, 11, 1, 9, 0)));
        assert_eq!(missed_run(&monthly(vec![1]), at(2026, 11, 3, 9, 30), last), None);
        // The last day a job on the 31st falls back to is caught up too.
        let march = Some(at(2027, 3, 31, 9, 0));
        assert_eq!(missed_run(&month_end(vec![31]), at(2027, 4, 30, 9, 30), march), Some(at(2027, 4, 30, 9, 0)));
    }

    #[test]
    fn a_paused_job_is_not_caught_up() {
        let paused = Job { enabled: false, ..job(9, 0, vec![]) };
        assert_eq!(missed_run(&paused, at(2026, 10, 27, 9, 16), None::<DateTime<Local>>), None);
    }

    #[test]
    fn paused_job_has_no_next_run() {
        let mut paused = job(9, 0, vec![]);
        paused.enabled = false;
        assert_eq!(next_run(&paused, at(2026, 10, 27, 8, 0)), None);
    }
}
