use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;
use tokio::sync::broadcast;
use truffle::session::PeerEvent;

/// Tray menu item IDs.
const MENU_QUIT: &str = "quit";

/// Build and register the system tray icon with context menu.
pub fn build_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let status_item = MenuItemBuilder::with_id("status", "Cheeseboard: Starting...")
        .enabled(false)
        .build(app)?;

    let devices_item = MenuItemBuilder::with_id("devices_header", "No devices")
        .enabled(false)
        .build(app)?;

    let quit_item = MenuItemBuilder::with_id(MENU_QUIT, "Quit Cheeseboard")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&status_item)
        .separator()
        .item(&devices_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let app_handle = app.clone();

    let mut builder = TrayIconBuilder::with_id("cheeseboard-tray")
        .menu(&menu)
        .tooltip("Cheeseboard - Clipboard Sync")
        .on_menu_event(move |_tray, event| {
            if event.id().as_ref() == MENU_QUIT {
                tracing::info!("Quit requested from tray menu");
                app_handle.exit(0);
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;

    Ok(())
}

/// Spawn a background task that updates the tray based on peer events.
pub fn spawn_tray_updater(_app_handle: AppHandle, mut event_rx: broadcast::Receiver<PeerEvent>) {
    tokio::spawn(async move {
        let mut peer_names: Vec<(String, String)> = Vec::new(); // (id, name)

        loop {
            match event_rx.recv().await {
                Ok(PeerEvent::Joined(state)) => {
                    if !peer_names.iter().any(|(id, _)| id == &state.id) {
                        peer_names.push((state.id.clone(), state.name.clone()));
                    }
                    update_status("Cheeseboard: Connected");
                    update_devices(&peer_names);
                }
                Ok(PeerEvent::Left(id)) => {
                    peer_names.retain(|(pid, _)| pid != &id);
                    update_devices(&peer_names);
                }
                Ok(PeerEvent::Updated(state)) => {
                    if let Some(entry) = peer_names.iter_mut().find(|(id, _)| id == &state.id) {
                        entry.1 = state.name.clone();
                    }
                    update_devices(&peer_names);
                }
                Ok(PeerEvent::Connected(id)) => {
                    tracing::info!("Peer connected: {id}");
                }
                Ok(PeerEvent::Disconnected(id)) => {
                    tracing::info!("Peer disconnected: {id}");
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Tray event receiver lagged by {n}");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

fn update_status(text: &str) {
    tracing::info!("Tray status: {text}");
}

fn update_devices(peers: &[(String, String)]) {
    if peers.is_empty() {
        tracing::info!("Tray devices: none");
    } else {
        let names: Vec<&str> = peers.iter().map(|(_, n)| n.as_str()).collect();
        tracing::info!("Tray devices: {}", names.join(", "));
    }
}
