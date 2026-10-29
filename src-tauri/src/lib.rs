pub mod cli;
pub mod commands;
pub mod gcloud;
pub mod interactive;
pub mod jobs;
pub mod launchd;
pub mod paths;
pub mod runner;
pub mod runs;
pub mod schedule;
pub mod tmux;
pub mod trust;
pub mod update;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|_app| {
            // A dev build must not point the real schedules at `target/debug`.
            if !cfg!(debug_assertions) {
                commands::sync_launchd();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_jobs,
            commands::defaults,
            commands::save_job,
            commands::delete_job,
            commands::set_enabled,
            commands::run_now,
            commands::watch_run,
            commands::list_runs,
            commands::get_run,
            commands::gcloud_status,
            commands::gcloud_sign_in,
            commands::tmux_status,
            commands::install_tmux,
            commands::app_version,
            commands::check_update,
            commands::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("Claude Jobs could not start");
}
