//! The agents on macOS: launchd agents, each a file `~/Library/LaunchAgents/com.claude-jobs.<id>.plist` loaded in the
//! user's GUI domain.

use crate::agents::{label, Agent, CalendarEntry, Start};
use crate::paths;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn agents_dir() -> PathBuf {
    paths::home().join("Library").join("LaunchAgents")
}

/// The agent's launchd file.
pub fn path(id: &str) -> PathBuf {
    agents_dir().join(format!("{}.plist", label(id)))
}

fn target(id: &str) -> String {
    format!("{}/{}", domain(), label(id))
}

fn domain() -> String {
    format!("gui/{}", unsafe { libc::getuid() })
}

fn launchctl(args: &[&str]) -> Option<Output> {
    Command::new("launchctl").args(args).output().ok()
}

pub fn is_loaded(id: &str) -> bool {
    launchctl(&["print", &target(id)]).is_some_and(|o| o.status.success())
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Plist {
    label: String,
    program_arguments: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_calendar_interval: Option<Vec<CalendarEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_interval: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    run_at_load: bool,
    environment_variables: BTreeMap<String, String>,
    standard_out_path: String,
    standard_error_path: String,
    process_type: String,
}

/// The agent's launchd file, as `load` writes it.
fn xml(agent: &Agent) -> Result<Vec<u8>, String> {
    let (calendar, interval, run_at_load) = match &agent.start {
        Start::Calendar(entries) => (Some(entries.clone()), None, false),
        Start::Login => (None, None, true),
        Start::Every(seconds) => (None, Some(*seconds), true),
    };
    let plist = Plist {
        label: label(&agent.id),
        program_arguments: agent.args.clone(),
        start_calendar_interval: calendar,
        start_interval: interval,
        run_at_load,
        environment_variables: agent.env.clone(),
        standard_out_path: agent.log.clone(),
        standard_error_path: agent.log.clone(),
        process_type: "Background".into(),
    };
    let mut xml = Vec::new();
    plist::to_writer_xml(&mut xml, &plist).map_err(|e| format!("cannot write the launchd file: {e}"))?;
    Ok(xml)
}

/// Writes the agent's launchd file and loads it in place of the one before. An agent that starts at login or at an
/// interval also runs once now.
pub fn load(agent: &Agent) -> Result<(), String> {
    let _ = launchctl(&["bootout", &target(&agent.id)]);
    fs::create_dir_all(agents_dir()).map_err(|e| e.to_string())?;
    let path = path(&agent.id);
    fs::write(&path, xml(agent)?).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let lint = Command::new("plutil").arg("-lint").arg(&path).output().map_err(|e| e.to_string())?;
    if !lint.status.success() {
        return Err(format!("the launchd file is invalid: {}", String::from_utf8_lossy(&lint.stdout).trim()));
    }
    let out = launchctl(&["bootstrap", &domain(), &path.display().to_string()]).ok_or("cannot run launchctl")?;
    if !out.status.success() && !is_loaded(&agent.id) {
        return Err(format!("launchctl bootstrap failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

/// Whether the agent is loaded from the very file `load` would write for it.
pub fn is_current(agent: &Agent) -> bool {
    xml(agent).is_ok_and(|xml| fs::read(path(&agent.id)).is_ok_and(|on_disk| on_disk == xml)) && is_loaded(&agent.id)
}

pub fn remove(id: &str) {
    let _ = launchctl(&["bootout", &target(id)]);
    let _ = fs::remove_file(path(id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::tests::{cron, entry};

    fn agent(start: Start) -> Agent {
        Agent {
            id: "t".into(),
            args: vec!["/Applications/Claude Jobs.app/Contents/MacOS/claude-jobs".into(), "poll".into()],
            start,
            env: BTreeMap::from([("PATH".to_string(), "/usr/bin:/bin".to_string())]),
            log: "/tmp/t.launchd.log".into(),
        }
    }

    fn text(agent: &Agent) -> String {
        String::from_utf8(xml(agent).expect("serializes")).expect("UTF-8")
    }

    #[test]
    fn the_events_agent_checks_at_its_interval_from_load() {
        let xml = text(&agent(Start::Every(300)));
        assert!(xml.contains("<key>StartInterval</key>\n\t<integer>300</integer>"), "{xml}");
        assert!(xml.contains("<key>RunAtLoad</key>") && xml.contains("<string>poll</string>"), "{xml}");
        assert!(!xml.contains("StartCalendarInterval"));
    }

    #[test]
    fn the_login_agent_runs_at_load_only() {
        let xml = text(&agent(Start::Login));
        assert!(xml.contains("<key>RunAtLoad</key>"), "{xml}");
        assert!(!xml.contains("StartInterval") && !xml.contains("StartCalendarInterval"), "{xml}");
    }

    #[test]
    fn a_calendar_entry_leaves_out_the_keys_that_match_any_value() {
        let xml = text(&agent(Start::Calendar(vec![entry(Some(15), None, Some(9), Some(30))])));
        assert!(xml.contains("<key>Day</key>\n\t\t\t<integer>15</integer>"), "{xml}");
        assert!(!xml.contains("Weekday") && !xml.contains("Month") && !xml.contains("RunAtLoad"), "{xml}");
        let busy = text(&agent(Start::Calendar(cron("*/2 9-17 * * 1-5"))));
        assert!(!busy.contains("Minute"), "{busy}");
    }
}
