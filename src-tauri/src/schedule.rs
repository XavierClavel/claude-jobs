//! When a job runs next, in local time, as launchd computes it from `StartCalendarInterval`.

use crate::jobs::Job;
use chrono::{DateTime, Datelike, Days, Local, TimeZone};

pub fn next_run(job: &Job, now: DateTime<Local>) -> Option<DateTime<Local>> {
    if !job.enabled {
        return None;
    }
    for offset in 0..8 {
        let day = now.date_naive().checked_add_days(Days::new(offset))?;
        if !job.weekdays.is_empty() && !job.weekdays.contains(&(day.weekday().number_from_monday() as u8)) {
            continue;
        }
        let at = day.and_hms_opt(job.hour.into(), job.minute.into(), 0)?;
        // On the night clocks go forward, a time that does not exist moves to the next valid one.
        let Some(candidate) = Local.from_local_datetime(&at).earliest() else { continue };
        if candidate > now {
            return Some(candidate);
        }
    }
    None
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
    fn paused_job_has_no_next_run() {
        let mut paused = job(9, 0, vec![]);
        paused.enabled = false;
        assert_eq!(next_run(&paused, at(2026, 10, 27, 8, 0)), None);
    }
}
