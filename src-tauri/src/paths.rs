//! Where Claude Jobs keeps its files. `~/.claude-jobs` is shared with the earlier Python version, so its jobs and
//! run history carry over. Also where the app's binary, the user's shell and Homebrew are, which differ between macOS
//! and Linux.

use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

pub fn base() -> PathBuf {
    home().join(".claude-jobs")
}

pub fn jobs_file() -> PathBuf {
    base().join("jobs.json")
}

/// The last check of each job condition: see `conditions`.
pub fn conditions_file() -> PathBuf {
    base().join("conditions.json")
}

/// The command conditions: see `conditions::CommandCondition`.
pub fn command_conditions_file() -> PathBuf {
    base().join("command_conditions.json")
}

/// What jobs do when a run ends: see `actions::Action`.
pub fn actions_file() -> PathBuf {
    base().join("actions.json")
}

pub fn layout_file() -> PathBuf {
    base().join("layout.json")
}

/// What each job an event starts has seen: see `events`.
pub fn events_file() -> PathBuf {
    base().join("events.json")
}

pub fn runs_dir() -> PathBuf {
    base().join("runs")
}

pub fn logs_dir() -> PathBuf {
    base().join("logs")
}

/// The working folder of a job that needs no particular one: empty, so a session there reads nothing else.
pub fn workspace() -> PathBuf {
    base().join("workspace")
}

/// What an agent's run printed, for a job or a shared agent (`app.catch-up`, `app.events`): see `agents`.
pub fn agent_log(id: &str) -> PathBuf {
    let scheduler = if cfg!(target_os = "linux") { "systemd" } else { "launchd" };
    logs_dir().join(format!("{id}.{scheduler}.log"))
}

/// This app's binary, which the agents and the runs' MCP servers call. An AppImage runs from a mount that changes at
/// each start: the AppImage itself is what lasts.
pub fn exe() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage));
    }
    std::env::current_exe().map_err(|e| format!("cannot find the app's binary: {e}"))
}

/// The shell the runs get as SHELL, and whose login PATH the jobs start from: zsh, macOS's own.
#[cfg(target_os = "macos")]
pub fn shell() -> String {
    "/bin/zsh".to_string()
}

/// The shell the runs get as SHELL, and whose login PATH the jobs start from: the user's, from the password database,
/// since a timer's run and one started from the window do not have the same environment.
#[cfg(target_os = "linux")]
pub fn shell() -> String {
    let entry = unsafe { libc::getpwuid(libc::getuid()) };
    let shell = if entry.is_null() {
        String::new()
    } else {
        unsafe { std::ffi::CStr::from_ptr((*entry).pw_shell) }.to_string_lossy().into_owned()
    };
    if std::path::Path::new(&shell).is_file() {
        shell
    } else {
        "/bin/bash".to_string()
    }
}

/// A program the app runs, other than itself. An AppImage starts the app with its own libraries, GTK and commands
/// first in the environment (`LD_LIBRARY_PATH`, `PATH`, `XDG_DATA_DIRS`, `GTK_PATH`…), which another program must not
/// load: they are left out of its environment, and the rest is as the app got it.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    if let (Some(_), Ok(appdir)) = (std::env::var_os("APPIMAGE"), std::env::var("APPDIR")) {
        for (name, value) in without_appdir(std::env::vars(), &appdir) {
            match value {
                Some(value) => cmd.env(name, value),
                None => cmd.env_remove(name),
            };
        }
    }
    cmd
}

/// The variables of `vars` that name something in `appdir`, the AppImage's mount: each with what is left of it once
/// those are taken out, or None when nothing is. `GTK_THEME` too, which the AppImage sets for its own GTK.
fn without_appdir(vars: impl Iterator<Item = (String, String)>, appdir: &str) -> Vec<(String, Option<String>)> {
    vars.filter(|(name, value)| value.contains(appdir) || name == "GTK_THEME")
        .map(|(name, value)| {
            let kept: Vec<&str> = value.split(':').filter(|part| !part.starts_with(appdir)).collect();
            let left = (name != "GTK_THEME" && kept.iter().any(|part| !part.is_empty())).then(|| kept.join(":"));
            (name, left)
        })
        .collect()
}

/// Where Homebrew puts the commands it installs: on Apple silicon, on Intel, then on Linux.
pub const HOMEBREW_BINS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/home/linuxbrew/.linuxbrew/bin"];

/// How to install `package`, a command jobs need, as the end of a sentence: `run brew install tmux`.
pub fn install_hint(package: &str) -> String {
    if cfg!(target_os = "linux") {
        format!("install {package} with your package manager (sudo apt install {package})")
    } else {
        format!("run brew install {package}")
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    match path.strip_prefix("~") {
        Some(rest) => home().join(rest.trim_start_matches('/')),
        None => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_appimage_adds_to_the_environment_is_taken_out() {
        let appdir = "/tmp/.mount_ClaudeX1";
        let vars = [
            ("PATH", format!("{appdir}/usr/bin/:{appdir}/usr/sbin/:/home/x/.local/bin:/usr/bin")),
            ("LD_LIBRARY_PATH", format!("{appdir}/usr/lib/:{appdir}/lib/:")),
            ("XDG_DATA_DIRS", format!("{appdir}/usr/share:/usr/share:/usr/local/share")),
            ("GTK_PATH", format!("{appdir}//usr/lib/gtk-3.0")),
            ("APPDIR", appdir.to_string()),
            ("GTK_THEME", "Adwaita:light".to_string()),
            ("HOME", "/home/x".to_string()),
            ("APPIMAGE", "/home/x/Applications/Claude-Jobs.AppImage".to_string()),
        ];
        let changed = without_appdir(vars.into_iter().map(|(k, v)| (k.to_string(), v)), appdir);
        let expected = |name: &str, value: Option<&str>| (name.to_string(), value.map(str::to_string));
        assert_eq!(
            changed,
            [
                expected("PATH", Some("/home/x/.local/bin:/usr/bin")),
                expected("LD_LIBRARY_PATH", None),
                expected("XDG_DATA_DIRS", Some("/usr/share:/usr/local/share")),
                expected("GTK_PATH", None),
                expected("APPDIR", None),
                expected("GTK_THEME", None),
            ]
        );
    }
}
