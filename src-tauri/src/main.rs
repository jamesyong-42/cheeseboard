// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod config;
mod tray;

use std::path::PathBuf;
use std::sync::Arc;

use clipboard::monitor::ClipboardMonitor;
use clipboard::store::ClipboardHistoryStore;
use clipboard::thread::ClipboardThread;
use config::AppConfig;
use tauri::Manager;
use truffle::NodeBuilder;

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
        .setup(|app| {
            let app_handle = app.handle().clone();

            tray::build_tray(&app_handle)?;

            let handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = async_setup(handle).await {
                    tracing::error!("Setup failed: {e}");
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Cheeseboard");
}

async fn async_setup(app_handle: tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Load config
    let config = AppConfig::load_or_create()?;
    tracing::info!("Device: {} ({})", config.device_name, config.device_id);

    // Step 2: Resolve sidecar binary path
    let sidecar_path = resolve_sidecar_path(&app_handle)?;

    // Step 3: State directory for tsnet
    let state_dir = AppConfig::state_dir()?;
    std::fs::create_dir_all(&state_dir)?;
    let state_dir_str = state_dir.to_string_lossy().to_string();

    // Step 4: Build truffle Node (handles sidecar, bridge, transport internally)
    let hostname = format!(
        "cheeseboard-{}",
        &config.device_id[..config.device_id.len().min(8)]
    );
    let node = Arc::new(
        NodeBuilder::default()
            .name(&hostname)
            .sidecar_path(&sidecar_path)
            .state_dir(&state_dir_str)
            .build()
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
    tray::spawn_tray_updater(app_handle, peer_rx);

    // Keep node alive for the lifetime of the app.
    std::mem::forget(node);

    tracing::info!("Cheeseboard setup complete");

    Ok(())
}

/// Resolve the path to the truffle sidecar binary.
///
/// Search order:
/// 1. Tauri resource directory (bundled with app)
/// 2. truffle::sidecar_path() (build-time download from truffle-sidecar crate)
/// 3. binaries/ directory (development override)
fn resolve_sidecar_path(
    app_handle: &tauri::AppHandle,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // 1. Tauri-bundled: check resource directory
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        for name in &["truffle-sidecar", "sidecar-slim"] {
            let path = resource_dir.join(name);
            if path.exists() {
                tracing::info!("Using Tauri-bundled sidecar: {}", path.display());
                return Ok(path);
            }
        }
    }

    // 2. truffle-sidecar crate: build-time downloaded binary
    let crate_path = truffle::sidecar_path();
    if crate_path.exists() {
        tracing::info!(
            "Using truffle-sidecar crate binary: {}",
            crate_path.display()
        );
        return Ok(crate_path);
    }

    // 3. Development: binaries/ directory
    let dev_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("binaries");
    for name in &["truffle-sidecar", "sidecar-slim"] {
        let path = dev_dir.join(name);
        if path.exists() {
            tracing::info!("Using dev sidecar: {}", path.display());
            return Ok(path);
        }
    }

    // Fallback: PATH lookup
    tracing::warn!("Sidecar not found locally, falling back to PATH");
    Ok(crate_path)
}
