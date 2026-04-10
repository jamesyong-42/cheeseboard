// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod config;
mod tray;
mod updater;

use std::path::PathBuf;
use std::sync::Arc;

use clipboard::monitor::ClipboardMonitor;
use clipboard::store::ClipboardHistoryStore;
use clipboard::thread::ClipboardThread;
use config::AppConfig;
use tauri::{Emitter, Manager};
use truffle::NodeBuilder;

/// Type-erased wrapper to keep the truffle node alive via Tauri managed state.
struct NodeKeepAlive(#[allow(dead_code)] Box<dyn std::any::Any + Send + Sync>);

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cheeseboard=info,truffle=info,truffle_core=info".into()),
        )
        .init();

    tracing::info!("Cheeseboard v{} starting", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![open_url, close_onboarding])
        .setup(|app| {
            let app_handle = app.handle().clone();

            let tray_items = tray::build_tray(&app_handle)?;

            // Spawn background update-check loop (20s delay, 6h interval)
            updater::spawn_update_loop(app_handle.clone());

            // Show onboarding window on first launch (or if no auth state)
            if needs_onboarding() {
                if let Some(win) = app.get_webview_window("onboarding") {
                    let _ = win.show();
                }
            }

            let handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let error_handle = handle.clone();
                if let Err(e) = async_setup(handle, tray_items).await {
                    tracing::error!("Setup failed: {e}");
                    emit_status(&error_handle, "error", None, &[]);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Cheeseboard");
}

/// Check if this is first launch (no tsnet state dir yet).
fn needs_onboarding() -> bool {
    match AppConfig::state_dir() {
        Ok(dir) => !dir.join("key.json").exists(),
        Err(_) => true,
    }
}

#[tauri::command]
fn open_url(url: String) {
    let _ = open::that(&url);
}

#[tauri::command]
fn close_onboarding(app: tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("onboarding") {
        let _ = win.hide();
    }
}

/// Emit a status event to the onboarding window.
fn emit_status(app: &tauri::AppHandle, state: &str, auth_url: Option<&str>, devices: &[String]) {
    let _ = app.emit(
        "cheeseboard://status",
        serde_json::json!({
            "state": state,
            "auth_url": auth_url,
            "devices": devices,
        }),
    );
}

async fn async_setup(
    app_handle: tauri::AppHandle,
    tray_items: tray::TrayMenuItems,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Load config
    let config = AppConfig::load_or_create()?;
    tracing::info!("Device: {} ({})", config.device_name, config.device_id);

    // Step 2: Resolve sidecar binary path
    let sidecar_path = resolve_sidecar_path(&app_handle)?;
    tracing::info!("Sidecar: {}", sidecar_path.display());

    // Step 3: State directory for tsnet
    let state_dir = AppConfig::state_dir()?;
    std::fs::create_dir_all(&state_dir)?;
    let state_dir_str = state_dir.to_string_lossy().to_string();

    emit_status(&app_handle, "connecting", None, &[]);

    // Step 4: Build truffle Node (with auth handler for first-run login)
    let hostname = format!(
        "cheeseboard-{}",
        &config.device_id[..config.device_id.len().min(8)]
    );
    let auth_handle = app_handle.clone();
    let node = Arc::new(
        NodeBuilder::default()
            .name(&hostname)
            .sidecar_path(&sidecar_path)
            .state_dir(&state_dir_str)
            .build_with_auth_handler(move |url| {
                tray::emit_auth_required(&auth_handle, &url);
            })
            .await?,
    );

    // Step 5: Create clipboard store + thread
    let clipboard_store = Arc::new(ClipboardHistoryStore::new(config.device_id.clone()));
    let clipboard_thread = ClipboardThread::spawn()?;

    // Step 6: Start clipboard monitor
    let monitor = ClipboardMonitor::new(
        clipboard_thread,
        Arc::clone(&clipboard_store),
        Arc::clone(&node),
        config.device_id,
        config.device_name,
    );
    tokio::spawn(monitor.run());

    // Step 7: Spawn tray updater with peer events
    let peer_rx = node.on_peer_change();
    tray::spawn_tray_updater(app_handle.clone(), peer_rx, tray_items);

    // Connected -- notify the onboarding window
    emit_status(&app_handle, "connected", None, &[]);

    // Keep the node alive for the lifetime of the app via Tauri managed state.
    app_handle.manage(NodeKeepAlive(Box::new(node)));

    tracing::info!("Cheeseboard setup complete");

    Ok(())
}

/// Resolve the path to the truffle sidecar binary.
///
/// Checks Tauri resource dir first (for bundled apps), then delegates to
/// truffle::sidecar_path() which checks exe dir, config dir, system paths,
/// and the build-time downloaded binary.
fn resolve_sidecar_path(
    app_handle: &tauri::AppHandle,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Tauri-bundled: check resource directory (for packaged .dmg/.exe/.deb)
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        for name in &["truffle-sidecar", "sidecar-slim"] {
            let path = resource_dir.join(name);
            if path.exists() {
                tracing::info!("Using Tauri-bundled sidecar: {}", path.display());
                return Ok(path);
            }
        }
    }

    // Delegate to truffle's smart sidecar resolution
    // (exe dir -> config dir -> system paths -> build-time download -> PATH)
    let path = truffle::sidecar_path();
    tracing::info!("Using sidecar: {}", path.display());
    Ok(path)
}
