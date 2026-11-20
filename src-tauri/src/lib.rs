pub mod actions;
pub mod agents;
pub mod cli;
pub mod commands;
pub mod conditions;
pub mod cron;
pub mod events;
pub mod gcloud;
pub mod interactive;
pub mod jobs;
#[cfg(target_os = "macos")]
pub mod launchd;
pub mod layout;
pub mod mcp;
pub mod offer;
pub mod paths;
pub mod runner;
pub mod runs;
pub mod schedule;
// Built for the tests on macOS too, so they check the units it writes there as well.
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub mod systemd;
pub mod tmux;
pub mod trust;
pub mod update;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|_app| {
            // A dev build must not point the real schedules at `target/debug`.
            if !cfg!(debug_assertions) {
                if let Ok(exe) = paths::exe() {
                    agents::sync(&exe);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_jobs,
            commands::get_layout,
            commands::save_layout,
            commands::defaults,
            commands::save_job,
            commands::cron_preview,
            commands::delete_job,
            commands::set_enabled,
            commands::run_now,
            commands::watch_run,
            commands::list_runs,
            commands::get_run,
            commands::gcloud_status,
            commands::gcloud_sign_in,
            commands::claude_status,
            commands::claude_sign_in,
            commands::list_conditions,
            commands::save_condition,
            commands::delete_condition,
            commands::check_condition,
            commands::list_repos,
            commands::open_on_github,
            commands::list_actions,
            commands::save_action,
            commands::delete_action,
            commands::test_action,
            commands::tmux_status,
            commands::install_tmux,
            commands::list_offer,
            commands::app_version,
            commands::check_update,
            commands::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("Claude Jobs could not start");
}
