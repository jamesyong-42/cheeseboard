use tauri::menu::{MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;
use tauri::{Emitter, Manager};
use tokio::sync::broadcast;
use truffle::session::PeerEvent;

/// Tray menu item IDs.
const MENU_QUIT: &str = "quit";
const MENU_CHECK_UPDATE: &str = "check_update";

/// Handles to tray menu items that need dynamic updates.
pub struct TrayMenuItems {
    status: MenuItem<tauri::Wry>,
    devices: MenuItem<tauri::Wry>,
}

/// Build and register the system tray icon with context menu.
/// Returns handles to updatable menu items.
pub fn build_tray(app: &AppHandle) -> Result<TrayMenuItems, Box<dyn std::error::Error>> {
    let status_item = MenuItemBuilder::with_id("status", "Cheeseboard: Starting...")
        .enabled(false)
        .build(app)?;

    let devices_item = MenuItemBuilder::with_id("devices_header", "No devices")
        .enabled(false)
        .build(app)?;

    let update_item =
        MenuItemBuilder::with_id(MENU_CHECK_UPDATE, "Check for updates…").build(app)?;

    let quit_item = MenuItemBuilder::with_id(MENU_QUIT, "Quit Cheeseboard")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&status_item)
        .separator()
        .item(&devices_item)
        .separator()
        .item(&update_item)
        .item(&quit_item)
        .build()?;

    let app_handle = app.clone();

    let mut builder = TrayIconBuilder::with_id("cheeseboard-tray")
        .menu(&menu)
        .tooltip("Cheeseboard - Clipboard Sync")
        .on_menu_event(move |_tray, event| match event.id().as_ref() {
            MENU_QUIT => {
                tracing::info!("Quit requested from tray menu");
                app_handle.exit(0);
            }
            MENU_CHECK_UPDATE => {
                let handle = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    crate::updater::check_and_install(handle).await;
                });
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;

    Ok(TrayMenuItems {
        status: status_item,
        devices: devices_item,
    })
}

/// Emit auth-required status to the onboarding window.
pub fn emit_auth_required(app: &AppHandle, url: &str) {
    tracing::info!("Tailscale auth required: {url}");
    if let Some(win) = app.get_webview_window("onboarding") {
        let _ = win.show();
        let _ = win.set_focus();
    }
    let _ = app.emit(
        "cheeseboard://status",
        serde_json::json!({
            "state": "auth_required",
            "auth_url": url,
            "devices": [],
        }),
    );
}

/// Spawn a background task that updates the tray based on peer events.
pub fn spawn_tray_updater(
    app_handle: AppHandle,
    mut event_rx: broadcast::Receiver<PeerEvent>,
    tray_items: TrayMenuItems,
) {
    tokio::spawn(async move {
        let mut peer_names: Vec<(String, String)> = Vec::new(); // (id, name)

        loop {
            match event_rx.recv().await {
                Ok(PeerEvent::Joined(state)) => {
                    if !peer_names.iter().any(|(id, _)| id == &state.id) {
                        peer_names.push((state.id.clone(), state.name.clone()));
                    }
                    let _ = tray_items.status.set_text("Cheeseboard: Connected");
                    update_devices_menu(&tray_items.devices, &peer_names);
                    emit_connected(&app_handle, &peer_names);
                }
                Ok(PeerEvent::Left(id)) => {
                    peer_names.retain(|(pid, _)| pid != &id);
                    update_devices_menu(&tray_items.devices, &peer_names);
                    emit_connected(&app_handle, &peer_names);
                }
                Ok(PeerEvent::Updated(state)) => {
                    if let Some(entry) = peer_names.iter_mut().find(|(id, _)| id == &state.id) {
                        entry.1 = state.name.clone();
                    }
                    update_devices_menu(&tray_items.devices, &peer_names);
                }
                Ok(PeerEvent::WsConnected(id)) => {
                    tracing::info!("Peer connected: {id}");
                }
                Ok(PeerEvent::WsDisconnected(id)) => {
                    tracing::info!("Peer disconnected: {id}");
                }
                Ok(PeerEvent::AuthRequired { url }) => {
                    let _ = tray_items.status.set_text("Cheeseboard: Auth Required");
                    emit_auth_required(&app_handle, &url);
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Tray event receiver lagged by {n}");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn emit_connected(app: &AppHandle, peers: &[(String, String)]) {
    let names: Vec<&str> = peers.iter().map(|(_, n)| n.as_str()).collect();
    let _ = app.emit(
        "cheeseboard://status",
        serde_json::json!({
            "state": "connected",
            "devices": names,
        }),
    );
}

fn update_devices_menu(devices_item: &MenuItem<tauri::Wry>, peers: &[(String, String)]) {
    let text = if peers.is_empty() {
        "No devices".to_string()
    } else {
        let names: Vec<&str> = peers.iter().map(|(_, n)| n.as_str()).collect();
        format!("Devices: {}", names.join(", "))
    };
    let _ = devices_item.set_text(&text);
}
