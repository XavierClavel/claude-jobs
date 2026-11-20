//! The agents on Linux: systemd user units in `~/.config/systemd/user`. Each agent is a service,
//! `com.claude-jobs.<id>.service`, that calls the app's binary. A timer of the same name starts it at a job's times,
//! or every 5 minutes for the events check; the login catch-up is a service the user's manager starts at login.

use crate::agents::{label, Agent, CalendarEntry, Start};
use crate::paths;
use std::fs;
use std::path::PathBuf;
use std::process::Output;

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

fn units_dir() -> PathBuf {
    paths::home().join(".config").join("systemd").join("user")
}

fn service(id: &str) -> String {
    format!("{}.service", label(id))
}

fn timer(id: &str) -> String {
    format!("{}.timer", label(id))
}

/// The agent's service file, which every agent has.
pub fn path(id: &str) -> PathBuf {
    units_dir().join(service(id))
}

fn timer_path(id: &str) -> PathBuf {
    units_dir().join(timer(id))
}

fn systemctl(args: &[&str]) -> Option<Output> {
    let mut cmd = paths::command("systemctl");
    cmd.arg("--user").args(args);
    // A run's environment has no XDG_RUNTIME_DIR, without which systemctl cannot reach the user's manager.
    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        cmd.env("XDG_RUNTIME_DIR", format!("/run/user/{}", unsafe { libc::getuid() }));
    }
    cmd.output().ok()
}

fn run(args: &[&str]) -> Result<(), String> {
    let out = systemctl(args).ok_or("cannot run systemctl: Claude Jobs needs systemd's user manager")?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("systemctl --user {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// A timer is loaded while it waits for its next time; the login service while it is enabled.
pub fn is_loaded(id: &str) -> bool {
    let check = if timer_path(id).exists() {
        ["is-active", "--quiet", &timer(id)]
    } else {
        ["is-enabled", "--quiet", &service(id)]
    };
    systemctl(&check).is_some_and(|o| o.status.success())
}

/// A word systemd reads back as `text`: quoted, with `%`, its specifiers' mark, doubled.
fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%"))
}

/// `Mon *-*-* 09:30:00`: the entry's day of the week, if any, then its date and time, `*` for any value.
fn on_calendar(entry: &CalendarEntry) -> String {
    let field = |value: Option<u8>| value.map_or("*".to_string(), |v| format!("{v:02}"));
    let weekday = entry.weekday.map(|d| format!("{} ", WEEKDAYS[usize::from(d % 7)])).unwrap_or_default();
    format!("{weekday}*-{}-{} {}:{}:00", field(entry.month), field(entry.day), field(entry.hour), field(entry.minute))
}

/// The agent's service, and its timer unless it starts at login: the files `load` writes.
fn units(agent: &Agent) -> (String, Option<String>) {
    // ExecStart also expands `$`, which Environment does not.
    let exec: Vec<String> = agent.args.iter().map(|a| quote(a).replace('$', "$$")).collect();
    let env: Vec<String> = agent.env.iter().map(|(k, v)| quote(&format!("{k}={v}"))).collect();
    let log = agent.log.replace('%', "%%");
    let head = format!("[Unit]\nDescription=Claude Jobs {}\n", agent.id);
    // KillMode=process: the runs a catch-up or a check starts, and the alerts, outlive the service.
    let mut service = format!(
        "{head}\n[Service]\nType=oneshot\nExecStart={}\nEnvironment={}\nStandardOutput=append:{log}\n\
         StandardError=append:{log}\nKillMode=process\n",
        exec.join(" "),
        env.join(" ")
    );
    let times = match &agent.start {
        Start::Login => {
            service.push_str("\n[Install]\nWantedBy=default.target\n");
            return (service, None);
        }
        Start::Calendar(entries) => entries.iter().map(|e| format!("OnCalendar={}\n", on_calendar(e))).collect(),
        Start::Every(seconds) => format!("OnActiveSec=0\nOnUnitActiveSec={seconds}\n"),
    };
    // AccuracySec: at the minute, rather than within the minute after it that systemd allows itself by default.
    let timer = format!("{head}\n[Timer]\n{times}AccuracySec=1s\n\n[Install]\nWantedBy=timers.target\n");
    (service, Some(timer))
}

/// Writes the agent's units and starts it on them. An agent that starts at login or at an interval also runs once
/// now. A run going on carries on.
pub fn load(agent: &Agent) -> Result<(), String> {
    fs::create_dir_all(units_dir()).map_err(|e| e.to_string())?;
    let (service_unit, timer_unit) = units(agent);
    let write =
        |path: PathBuf, text: &str| fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()));
    write(path(&agent.id), &service_unit)?;
    match &timer_unit {
        Some(text) => write(timer_path(&agent.id), text)?,
        None => {
            let _ = fs::remove_file(timer_path(&agent.id));
        }
    }
    run(&["daemon-reload"])?;
    match timer_unit {
        Some(_) => {
            run(&["enable", &timer(&agent.id)])?;
            run(&["restart", &timer(&agent.id)])
        }
        None => {
            run(&["enable", &service(&agent.id)])?;
            run(&["start", "--no-block", &service(&agent.id)])
        }
    }
}

/// Whether the agent is loaded from the very units `load` would write for it.
pub fn is_current(agent: &Agent) -> bool {
    let (service_unit, timer_unit) = units(agent);
    fs::read_to_string(path(&agent.id)).ok() == Some(service_unit)
        && fs::read_to_string(timer_path(&agent.id)).ok() == timer_unit
        && is_loaded(&agent.id)
}

/// Stops the agent's timer and removes its units. A run going on carries on.
pub fn remove(id: &str) {
    if !path(id).exists() && !timer_path(id).exists() {
        return;
    }
    let _ = systemctl(&["disable", "--now", &timer(id)]);
    let _ = systemctl(&["disable", &service(id)]);
    let _ = fs::remove_file(timer_path(id));
    let _ = fs::remove_file(path(id));
    let _ = systemctl(&["daemon-reload"]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::tests::{cron, entry};
    use std::collections::BTreeMap;

    fn agent(start: Start) -> Agent {
        Agent {
            id: "t".into(),
            args: vec!["/home/x/Applications/Claude Jobs.AppImage".into(), "run".into(), "t".into()],
            start,
            env: BTreeMap::from([
                ("HOME".to_string(), "/home/x".to_string()),
                ("PATH".to_string(), "/home/x/.local/bin:/usr/bin".to_string()),
            ]),
            log: "/home/x/.claude-jobs/logs/t.systemd.log".into(),
        }
    }

    #[test]
    fn a_job_is_a_service_its_timer_starts_at_each_of_its_times() {
        let (service, timer) = units(&agent(Start::Calendar(vec![
            entry(None, Some(1), Some(9), Some(30)),
            entry(Some(15), None, Some(9), Some(30)),
        ])));
        assert!(service.contains("\nType=oneshot\n"), "{service}");
        assert!(
            service.contains("\nExecStart=\"/home/x/Applications/Claude Jobs.AppImage\" \"run\" \"t\"\n"),
            "{service}"
        );
        assert!(service.contains("\nEnvironment=\"HOME=/home/x\" \"PATH=/home/x/.local/bin:/usr/bin\"\n"), "{service}");
        assert!(service.contains("\nStandardOutput=append:/home/x/.claude-jobs/logs/t.systemd.log\n"), "{service}");
        assert!(service.contains("\nKillMode=process\n") && !service.contains("[Install]"), "{service}");
        let timer = timer.expect("a timer");
        assert!(timer.contains("\nOnCalendar=Mon *-*-* 09:30:00\nOnCalendar=*-*-15 09:30:00\n"), "{timer}");
        assert!(timer.contains("\nAccuracySec=1s\n") && timer.contains("\nWantedBy=timers.target\n"), "{timer}");
        assert!(!timer.contains("Persistent"), "{timer}");
    }

    #[test]
    fn calendar_entries_read_as_systemd_times() {
        let lines: Vec<String> = cron("*/30 9 * 6 0").iter().map(on_calendar).collect();
        assert_eq!(lines, ["Sun *-06-* 09:00:00", "Sun *-06-* 09:30:00"]);
        assert_eq!(
            on_calendar(&CalendarEntry { month: Some(2), ..entry(Some(28), None, Some(7), Some(5)) }),
            "*-02-28 07:05:00"
        );
        assert_eq!(on_calendar(&entry(None, None, None, Some(0))), "*-*-* *:00:00");
    }

    #[test]
    fn the_events_check_starts_when_loaded_then_at_its_interval() {
        let (_, timer) = units(&agent(Start::Every(300)));
        let timer = timer.expect("a timer");
        assert!(timer.contains("\nOnActiveSec=0\nOnUnitActiveSec=300\n") && !timer.contains("OnCalendar"), "{timer}");
    }

    #[test]
    fn the_catch_up_is_a_service_started_at_login() {
        let (service, timer) = units(&agent(Start::Login));
        assert_eq!(timer, None);
        assert!(service.ends_with("\n[Install]\nWantedBy=default.target\n"), "{service}");
    }

    #[test]
    fn what_systemd_would_expand_is_escaped() {
        assert_eq!(quote(r#"a "b" \c 100%"#), r#""a \"b\" \\c 100%%""#);
        let odd = Agent { args: vec!["/opt/$x/claude-jobs".into()], log: "/l/50%.log".into(), ..agent(Start::Login) };
        let (service, _) = units(&odd);
        assert!(
            service.contains("\nExecStart=\"/opt/$$x/claude-jobs\"\n") && service.contains("append:/l/50%%.log"),
            "{service}"
        );
    }
}
