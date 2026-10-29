//! New versions, from the GitHub releases (`plugins.updater` in `tauri.conf.json`). The window asks whether one is
//! out; installing it swaps the app bundle in place, so the launchd agents, which call the same path, run the new
//! version from their next run on.

use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};
use ts_rs::TS;

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[ts(export)]
pub struct AppUpdate {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

async fn newer<R: Runtime>(app: &AppHandle<R>) -> Result<Option<Update>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| format!("cannot check for updates: {e}"))
}

/// The newest release when it is newer than this app. A dev build never updates: it is not an app bundle.
pub async fn check<R: Runtime>(app: &AppHandle<R>) -> Result<Option<AppUpdate>, String> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    Ok(newer(app).await?.map(|u| AppUpdate { version: u.version, current_version: u.current_version, notes: u.body }))
}

/// Downloads the newest release, checks its signature, puts it in place of this app and restarts on it. Returns
/// only when that failed. Runs in progress are their own process groups and carry on.
pub async fn install<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err("A dev build does not update itself.".to_string());
    }
    let update = newer(app).await?.ok_or("Claude Jobs is already up to date.")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("The update to {} could not be installed: {e}", update.version))?;
    app.restart()
}
