//! Auto-update support.
//!
//! Two entry points:
//!
//! - `spawn_update_loop` — background task that checks every 6 hours
//!   (after a 20-second startup delay to let the network come up).
//!   Logs results; never panics.
//!
//! - `check_and_install` — user-triggered check from the tray menu.
//!   Runs a fresh check, downloads the update if available, and
//!   restarts the app.

use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const STARTUP_DELAY: Duration = Duration::from_secs(20);

/// Spawn the background update-check loop. Errors are logged and swallowed;
/// an update check failure must never crash the app.
pub fn spawn_update_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        loop {
            match check_silently(&app).await {
                Ok(Some(v)) => tracing::info!("updater: {v} available (visible via tray menu)"),
                Ok(None) => tracing::debug!("updater: up to date ({})", env!("CARGO_PKG_VERSION")),
                Err(e) => tracing::warn!("updater: background check failed: {e}"),
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

/// Check for updates without side effects. Returns the new version string
/// if an update is available.
async fn check_silently(app: &AppHandle) -> tauri_plugin_updater::Result<Option<String>> {
    let updater = app.updater()?;
    Ok(updater.check().await?.map(|u| u.version))
}

/// User-triggered check + download + install + restart.
/// Called from the tray menu handler; runs on the async runtime.
pub async fn check_and_install(app: AppHandle) {
    tracing::info!("updater: user-triggered check");

    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("updater: failed to access updater plugin: {e}");
            return;
        }
    };

    let maybe_update = match updater.check().await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("updater: check failed: {e}");
            return;
        }
    };

    let Some(update) = maybe_update else {
        tracing::info!(
            "updater: already up to date (v{})",
            env!("CARGO_PKG_VERSION")
        );
        return;
    };

    tracing::info!(
        "updater: installing v{} (from v{})",
        update.version,
        update.current_version
    );

    if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
        tracing::error!("updater: download_and_install failed: {e}");
        return;
    }

    tracing::info!("updater: install complete, restarting");
    app.restart();
}
