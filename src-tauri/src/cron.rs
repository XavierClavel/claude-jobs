//! Cron expressions, for a job whose times the days of the week or of the month cannot say: five fields, minute,
//! hour, day of the month, month and day of the week, read the way crontab(5) reads them. Each field is `*`, a number,
//! a range `a-b`, a step over either (`*/15`, `9-17/2`, `5/10` from 5 on), or a list of those (`1,15`); months and
//! days of the week take their names too (`jan`, `mon-fri`), and Sunday is 0 or 7. `@hourly`, `@daily`,
//! `@midnight`, `@weekly`, `@monthly`, `@yearly` and `@annually` stand for the expressions they name.

use chrono::{Datelike, NaiveDate};

/// A cron expression, read: the values each field allows, sorted.
#[derive(Debug, Clone, PartialEq)]
pub struct Cron {
    /// 0 to 59.
    pub minutes: Vec<u8>,
    /// 0 to 23.
    pub hours: Vec<u8>,
    /// 1 to 31.
    pub days: Vec<u8>,
    /// 1 to 12.
    pub months: Vec<u8>,
    /// 0 (Sunday) to 6.
    pub weekdays: Vec<u8>,
    /// Whether a day has to match both the day of the month and the day of the week. crontab(5) asks for both when
    /// either field starts with `*`, and for either one otherwise: `0 9 1 * 1` runs on the 1st and on every Monday.
    pub both_days: bool,
}

struct Field {
    name: &'static str,
    min: u8,
    max: u8,
    names: &'static [&'static str],
}

const MINUTE: Field = Field { name: "minute", min: 0, max: 59, names: &[] };
const HOUR: Field = Field { name: "hour", min: 0, max: 23, names: &[] };
const DAY: Field = Field { name: "day of the month", min: 1, max: 31, names: &[] };
const MONTH: Field = Field {
    name: "month",
    min: 1,
    max: 12,
    names: &["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"],
};
/// 7 is Sunday too, as in crontab(5).
const WEEKDAY: Field =
    Field { name: "day of the week", min: 0, max: 7, names: &["sun", "mon", "tue", "wed", "thu", "fri", "sat"] };

const MACROS: [(&str, &str); 7] = [
    ("@hourly", "0 * * * *"),
    ("@daily", "0 0 * * *"),
    ("@midnight", "0 0 * * *"),
    ("@weekly", "0 0 * * 0"),
    ("@monthly", "0 0 1 * *"),
    ("@yearly", "0 0 1 1 *"),
    ("@annually", "0 0 1 1 *"),
];

/// What the editor and `claude-jobs save` say about an expression that is not five fields.
pub const FIVE_FIELDS: &str = "A cron expression has five fields: minute, hour, day of the month, month and day of \
                               the week, such as */15 9-17 * * 1-5.";

/// The days in each month, February in a leap year.
const MONTH_LENGTHS: [u8; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

impl Field {
    fn value(&self, text: &str) -> Result<u8, String> {
        if text.is_empty() {
            return Err(format!("Cron: the {} has an empty item.", self.name));
        }
        let lower = text.to_ascii_lowercase();
        let named = self.names.iter().position(|n| *n == lower).map(|i| i as u8 + self.min);
        match named.or_else(|| text.parse::<u8>().ok().filter(|v| (self.min..=self.max).contains(v))) {
            Some(v) => Ok(v),
            None => {
                let names = match (self.names.first(), self.names.last()) {
                    (Some(first), Some(last)) => format!(" or {first} to {last}"),
                    _ => String::new(),
                };
                Err(format!("Cron: the {} must be {} to {}{names}, not {text}.", self.name, self.min, self.max))
            }
        }
    }

    /// The values `text` allows, sorted, and whether it starts with `*`.
    fn parse(&self, text: &str) -> Result<(Vec<u8>, bool), String> {
        let mut allowed = [false; 60];
        for item in text.split(',') {
            let (range, step) = match item.split_once('/') {
                Some((range, step)) => match step.parse::<u8>() {
                    Ok(step) if step > 0 => (range, Some(step)),
                    _ => return Err(format!("Cron: the step in {item} is not a number from 1.")),
                },
                None => (item, None),
            };
            let (start, end) = match range.split_once('-') {
                _ if range == "*" => (self.min, self.max),
                Some((a, b)) => match (self.value(a)?, self.value(b)?) {
                    // `fri-sun`: a week that ends on Sunday ends on its 7.
                    (start, 0) if self.max == 7 => (start, 7),
                    ends => ends,
                },
                // `5/10` steps from 5 to the end.
                None if step.is_some() => (self.value(range)?, self.max),
                None => {
                    let v = self.value(range)?;
                    (v, v)
                }
            };
            if start > end {
                return Err(format!(
                    "Cron: the {} range {range} goes backwards; write it as two, {start}-{},{}-{end}.",
                    self.name, self.max, self.min
                ));
            }
            for v in (start..=end).step_by(step.unwrap_or(1).into()) {
                allowed[usize::from(v)] = true;
            }
        }
        let values = (self.min..=self.max).filter(|&v| allowed[usize::from(v)]).collect();
        Ok((values, text.starts_with('*')))
    }
}

impl Cron {
    pub fn parse(expression: &str) -> Result<Cron, String> {
        let expression = expression.trim();
        let expression = match expression.strip_prefix('@') {
            Some(name) if name.eq_ignore_ascii_case("reboot") => {
                return Err("Cron: @reboot has no time to run at. Every job already catches up at login.".into())
            }
            Some(_) => {
                let found = MACROS.iter().find(|(name, _)| name.eq_ignore_ascii_case(expression));
                let names: Vec<&str> = MACROS.iter().map(|(name, _)| *name).collect();
                found
                    .map(|(_, expands)| *expands)
                    .ok_or_else(|| format!("Cron: {expression} is not one of {}.", names.join(", ")))?
            }
            None => expression,
        };
        let [minute, hour, day, month, weekday] = expression.split_whitespace().collect::<Vec<_>>()[..] else {
            return Err(FIVE_FIELDS.to_string());
        };
        let (days, any_day) = DAY.parse(day)?;
        let (weekdays, any_weekday) = WEEKDAY.parse(weekday)?;
        // Sunday is 0 or 7: one Sunday.
        let mut weekdays: Vec<u8> = weekdays.into_iter().map(|d| d % 7).collect();
        weekdays.sort_unstable();
        weekdays.dedup();
        let cron = Cron {
            minutes: MINUTE.parse(minute)?.0,
            hours: HOUR.parse(hour)?.0,
            days,
            months: MONTH.parse(month)?.0,
            weekdays,
            both_days: any_day || any_weekday,
        };
        // Any day of the week comes in every month; a day of the month does not: there is no 30 February.
        let has_a_day = cron.months.iter().any(|&m| cron.days.iter().any(|&d| d <= MONTH_LENGTHS[usize::from(m - 1)]));
        if cron.both_days && !has_a_day {
            return Err("Cron: this never runs, since none of its months has one of its days of the month.".into());
        }
        Ok(cron)
    }

    /// Whether it runs on `day`, at its hours and minutes.
    pub fn runs_on(&self, day: NaiveDate) -> bool {
        let date = self.days.contains(&(day.day() as u8));
        let weekday = self.weekdays.contains(&(day.weekday().num_days_from_sunday() as u8));
        self.months.contains(&(day.month() as u8)) && if self.both_days { date && weekday } else { date || weekday }
    }

    /// Its times in a day, earliest first, as hour and minute.
    pub fn times(&self) -> impl Iterator<Item = (u8, u8)> + '_ {
        self.hours.iter().flat_map(move |&h| self.minutes.iter().map(move |&m| (h, m)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cron(expression: &str) -> Cron {
        Cron::parse(expression).unwrap_or_else(|e| panic!("{expression}: {e}"))
    }

    fn error(expression: &str) -> String {
        Cron::parse(expression).expect_err(expression)
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
    }

    #[test]
    fn reads_numbers_ranges_steps_and_lists() {
        let every_quarter = cron("*/15 9-17 * * 1-5");
        assert_eq!(every_quarter.minutes, vec![0, 15, 30, 45]);
        assert_eq!(every_quarter.hours, (9..=17).collect::<Vec<_>>());
        assert_eq!(every_quarter.days, (1..=31).collect::<Vec<_>>());
        assert_eq!(every_quarter.months, (1..=12).collect::<Vec<_>>());
        assert_eq!(every_quarter.weekdays, vec![1, 2, 3, 4, 5]);
        assert_eq!(cron("5,0,30 8-20/4 1,15 * *").minutes, vec![0, 5, 30]);
        assert_eq!(cron("0 8-20/4 * * *").hours, vec![8, 12, 16, 20]);
        assert_eq!(cron("5/20 * * * *").minutes, vec![5, 25, 45]);
        assert_eq!(cron("0 0 */10 * *").days, vec![1, 11, 21, 31]);
    }

    #[test]
    fn reads_names_and_both_sundays() {
        let weekend = cron("0 10 * jan,JUL-sep Sat-sun");
        assert_eq!(weekend.months, vec![1, 7, 8, 9]);
        assert_eq!(weekend.weekdays, vec![0, 6]);
        assert_eq!(cron("0 10 * * 7").weekdays, vec![0]);
        assert_eq!(cron("0 10 * * 0-7").weekdays, (0..=6).collect::<Vec<_>>());
        assert_eq!(cron("0 10 * * mon-sun").weekdays, (0..=6).collect::<Vec<_>>());
        assert_eq!(cron("0 10 * * 5-0").weekdays, vec![0, 5, 6]);
    }

    #[test]
    fn reads_the_macros() {
        assert_eq!(cron("@daily"), cron("0 0 * * *"));
        assert_eq!(cron("@Weekly"), cron("0 0 * * 0"));
        assert_eq!(cron("@hourly").minutes, vec![0]);
        assert!(error("@reboot").contains("catches up at login"));
        assert!(error("@sometimes").contains("@hourly, @daily"));
    }

    #[test]
    fn says_what_is_wrong() {
        assert_eq!(error("0 9 * *"), FIVE_FIELDS);
        assert_eq!(error("0 0 9 * * 1"), FIVE_FIELDS);
        assert_eq!(error(""), FIVE_FIELDS);
        assert_eq!(error("60 9 * * *"), "Cron: the minute must be 0 to 59, not 60.");
        assert_eq!(error("0 9 * foo *"), "Cron: the month must be 1 to 12 or jan to dec, not foo.");
        assert_eq!(error("0 9 * * 8"), "Cron: the day of the week must be 0 to 7 or sun to sat, not 8.");
        assert_eq!(error("0 9 0 * *"), "Cron: the day of the month must be 1 to 31, not 0.");
        assert_eq!(error("-5 9 * * *"), "Cron: the minute has an empty item.");
        assert_eq!(error("0 22-6 * * *"), "Cron: the hour range 22-6 goes backwards; write it as two, 22-23,0-6.");
        assert_eq!(error("*/0 * * * *"), "Cron: the step in */0 is not a number from 1.");
        assert_eq!(error("0 9 1,,15 * *"), "Cron: the day of the month has an empty item.");
        assert!(error("0 9 30 2 *").contains("never runs"));
        assert!(error("0 9 31 4,6,9,11 *").contains("never runs"));
    }

    #[test]
    fn a_day_matches_either_field_unless_one_starts_with_a_star() {
        // 2026-11-02 is a Monday, 2026-11-01 a Sunday.
        let first_or_monday = cron("0 9 1 * 1");
        assert!(!first_or_monday.both_days);
        assert!(first_or_monday.runs_on(date(2026, 11, 1)) && first_or_monday.runs_on(date(2026, 11, 2)));
        assert!(!first_or_monday.runs_on(date(2026, 11, 3)));
        // Every other day of the month, on Mondays only.
        let odd_mondays = cron("0 9 */2 * 1");
        assert!(odd_mondays.both_days);
        assert!(!odd_mondays.runs_on(date(2026, 11, 2)) && odd_mondays.runs_on(date(2026, 11, 9)));
        assert!(!odd_mondays.runs_on(date(2026, 11, 11)));
        // Weekdays in June only.
        let june = cron("0 9 * 6 1-5");
        assert!(june.runs_on(date(2027, 6, 1)) && !june.runs_on(date(2027, 7, 1)) && !june.runs_on(date(2027, 6, 5)));
    }

    #[test]
    fn a_leap_day_runs() {
        let leap_day = cron("0 9 29 2 *");
        assert!(leap_day.runs_on(date(2028, 2, 29)));
    }

    #[test]
    fn times_go_through_hours_then_minutes() {
        assert_eq!(cron("0,30 9,17 * * *").times().collect::<Vec<_>>(), vec![(9, 0), (9, 30), (17, 0), (17, 30)]);
    }
}
