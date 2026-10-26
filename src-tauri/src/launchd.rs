//! Each enabled job is a launchd agent, `com.claude-jobs.<id>`, that calls this app's binary with `run <id>`.

use crate::jobs::Job;
use crate::paths;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LABEL_PREFIX: &str = "com.claude-jobs.";

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

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "PascalCase")]
struct CalendarEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    weekday: Option<u8>,
    hour: u8,
    minute: u8,
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
    start_calendar_interval: CalendarInterval,
    environment_variables: BTreeMap<String, String>,
    standard_out_path: String,
    standard_error_path: String,
    process_type: String,
}

fn interval(job: &Job) -> CalendarInterval {
    if job.weekdays.is_empty() {
        return CalendarInterval::Daily(CalendarEntry { weekday: None, hour: job.hour, minute: job.minute });
    }
    // launchd counts Sunday as 0, the editor as 7.
    CalendarInterval::Days(
        job.weekdays
            .iter()
            .map(|d| CalendarEntry { weekday: Some(d % 7), hour: job.hour, minute: job.minute })
            .collect(),
    )
}

fn write_plist(job: &Job, exe: &Path) -> Result<(), String> {
    fs::create_dir_all(paths::logs_dir()).map_err(|e| e.to_string())?;
    fs::create_dir_all(paths::agents_dir()).map_err(|e| e.to_string())?;
    let log = paths::logs_dir().join(format!("{}.launchd.log", job.id)).display().to_string();
    let agent = Agent {
        label: label(&job.id),
        program_arguments: vec![exe.display().to_string(), "run".into(), job.id.clone()],
        start_calendar_interval: interval(job),
        environment_variables: BTreeMap::from([
            ("PATH".to_string(), job.path.clone()),
            ("HOME".to_string(), paths::home().display().to_string()),
        ]),
        standard_out_path: log.clone(),
        standard_error_path: log,
        process_type: "Background".into(),
    };
    let path = plist_path(&job.id);
    plist::to_file_xml(&path, &agent).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let lint = Command::new("plutil").arg("-lint").arg(&path).output().map_err(|e| e.to_string())?;
    if !lint.status.success() {
        return Err(format!("the launchd file is invalid: {}", String::from_utf8_lossy(&lint.stdout).trim()));
    }
    Ok(())
}

/// Makes launchd match the job: loaded with its schedule when enabled, gone when paused.
pub fn apply(job: &Job, exe: &Path) -> Result<(), String> {
    let target = format!("{}/{}", domain(), label(&job.id));
    let _ = launchctl(&["bootout", &target]);
    if !job.enabled {
        let _ = fs::remove_file(plist_path(&job.id));
        return Ok(());
    }
    write_plist(job, exe)?;
    let path = plist_path(&job.id).display().to_string();
    let out = launchctl(&["bootstrap", &domain(), &path]).ok_or("cannot run launchctl")?;
    if !out.status.success() && !is_loaded(&job.id) {
        return Err(format!("launchctl bootstrap failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
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

    #[test]
    fn every_day_is_one_entry_without_a_weekday() {
        assert_eq!(
            interval(&job(vec![])),
            CalendarInterval::Daily(CalendarEntry { weekday: None, hour: 9, minute: 30 })
        );
    }

    #[test]
    fn sunday_is_zero_for_launchd() {
        let CalendarInterval::Days(days) = interval(&job(vec![1, 7])) else { panic!("expected days") };
        assert_eq!(days.iter().map(|d| d.weekday).collect::<Vec<_>>(), vec![Some(1), Some(0)]);
    }
}
