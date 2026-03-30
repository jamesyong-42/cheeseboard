# Cheeseboard Implementation Plan
**Status**: Ready for implementation
**Created**: 2026-03-16

---

# Cheeseboard MVP (v0.1) -- Implementation Plan

## 1. Project Scaffold

### 1.1 Create the project

Do NOT use `cargo create-tauri-app` -- it scaffolds a full windowed app with a heavy JS frontend. Instead, manually scaffold a minimal Tauri v2 tray-only app:

```
mkdir -p /Users/jamesyong/Projects/project100/p008/cheeseboard
cd /Users/jamesyong/Projects/project100/p008/cheeseboard
cargo init --name cheeseboard src-tauri
```

### 1.2 Directory structure

```
cheeseboard/
  src-tauri/
    Cargo.toml
    tauri.conf.json
    capabilities/
      default.json
    icons/
      tray-connected.png       (32x32 template icon)
      tray-disconnected.png    (32x32 template icon)
    src/
      main.rs
      sidecar.rs
      mesh.rs
      config.rs
      tray.rs
      clipboard/
        mod.rs
        thread.rs
        store.rs
        monitor.rs
    build.rs
  src/                          (minimal frontend -- just an index.html)
    index.html
  binaries/                     (sidecar goes here for Tauri bundling)
    sidecar-slim-aarch64-apple-darwin
```

### 1.3 `src-tauri/Cargo.toml`

```toml
[package]
name = "cheeseboard"
version = "0.1.0"
edition = "2021"

[dependencies]
truffle-core = { path = "../../truffle/crates/truffle-core" }
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt-multi-thread", "sync", "macros", "time"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
arboard = "3"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
uuid = { version = "1", features = ["v4"] }
directories = "5"
rand = "0.9"
open = "5"

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

### 1.4 `src-tauri/tauri.conf.json`

```json
{
  "$schema": "https://raw.githubusercontent.com/nickelpack/tauri/v2/crates/tauri-cli/schema.json",
  "productName": "Cheeseboard",
  "version": "0.1.0",
  "identifier": "com.cheeseboard.app",
  "build": {
    "frontendDist": "../src"
  },
  "app": {
    "withGlobalTauri": false,
    "windows": [],
    "trayIcon": {
      "iconPath": "icons/tray-disconnected.png",
      "iconAsTemplate": true,
      "tooltip": "Cheeseboard - Clipboard Sync"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["dmg"],
    "icon": ["icons/tray-connected.png"],
    "externalBin": ["../binaries/sidecar-slim"]
  }
}
```

Key points:
- `"windows": []` -- no windows at startup, tray-only.
- `externalBin` -- Tauri's sidecar mechanism. At build time it resolves to `binaries/sidecar-slim-aarch64-apple-darwin` on macOS arm64.
- `trayIcon` defined at top level so it exists at app launch.

### 1.5 `src-tauri/build.rs`

```rust
fn main() {
    tauri_build::build()
}
```

### 1.6 `src/index.html`

```html
<!DOCTYPE html>
<html><body></body></html>
```

This file is required by Tauri's `frontendDist` but never displayed (no windows).

### 1.7 `src-tauri/capabilities/default.json`

```json
{
  "identifier": "default",
  "description": "Default capabilities for Cheeseboard",
  "windows": [],
  "permissions": [
    "core:default",
    "shell:allow-open"
  ]
}
```

---

## 2. Module Structure

### File: `src-tauri/src/main.rs`

**Purpose**: Tauri setup, state management, tray initialization, orchestration.

Key signatures:

```rust
// Application state shared across Tauri
struct AppState {
    mesh_node: Arc<MeshNode>,
    store_sync_adapter: Arc<StoreSyncAdapter>,
    clipboard_store: Arc<ClipboardHistoryStore>,
    go_shim: Arc<tokio::sync::Mutex<Option<GoShim>>>,
    config: AppConfig,
    // Join handles for cleanup
    sync_handles: Arc<tokio::sync::Mutex<Option<(JoinHandle<()>, JoinHandle<()>)>>>,
    monitor_shutdown: Arc<tokio::sync::Notify>,
}

fn main() {
    // 1. Init tracing
    // 2. Load or create config (config::load_or_create())
    // 3. Build Tauri app with .setup() callback
    // 4. In setup: spawn async init task that does:
    //    a. Start BridgeManager
    //    b. Start GoShim sidecar
    //    c. Create ConnectionManager, MeshNode
    //    d. Create ClipboardHistoryStore
    //    e. Create StoreSyncAdapter, wire_store_sync()
    //    f. Start clipboard monitor thread
    //    g. Listen for ShimLifecycleEvents (auth, peers, status)
    //    h. Build tray menu
    // 5. .on_exit() to clean up
}
```

### File: `src-tauri/src/sidecar.rs`

**Purpose**: GoShim + BridgeManager + ConnectionManager lifecycle management.

```rust
pub struct SidecarStack {
    pub bridge_manager_port: u16,
    pub session_token: [u8; 32],
    pub go_shim: GoShim,
    pub shim_lifecycle_rx: broadcast::Receiver<ShimLifecycleEvent>,
    pub connection_manager: Arc<ConnectionManager>,
    pub transport_event_rx: broadcast::Receiver<TransportEvent>,
}

/// Initialize the full sidecar stack.
/// 1. Generate session token (32 random bytes)
/// 2. BridgeManager::bind(token)
/// 3. ConnectionManager::new(config)
/// 4. Register WsIncomingHandler + WsOutgoingHandler on BridgeManager
/// 5. Spawn BridgeManager::run()
/// 6. Resolve sidecar binary path
/// 7. GoShim::spawn(ShimConfig { ... })
/// Returns SidecarStack with all handles.
pub async fn init_sidecar(
    config: &AppConfig,
) -> Result<SidecarStack, Box<dyn std::error::Error>>;

/// Spawn a task that listens for ShimLifecycleEvents and:
/// - On AuthRequired: open browser via `open::that(url)`
/// - On Status(running): call mesh_node.set_auth_authenticated()
/// - On Peers: call mesh_node.handle_tailnet_peers(), initiate dials
/// - On Crashed: update tray icon to disconnected
pub fn spawn_lifecycle_listener(
    mesh_node: Arc<MeshNode>,
    go_shim: Arc<tokio::sync::Mutex<Option<GoShim>>>,
    shim_lifecycle_rx: broadcast::Receiver<ShimLifecycleEvent>,
    tray_handle: TrayIconHandle,
);
```

### File: `src-tauri/src/clipboard/mod.rs`

```rust
pub mod thread;
pub mod store;
pub mod monitor;
```

### File: `src-tauri/src/clipboard/store.rs`

**Purpose**: `ClipboardHistoryStore` implementing `SyncableStore`.

```rust
use std::sync::RwLock;  // NOT tokio::sync::RwLock -- per SyncableStore docs
use truffle_core::store_sync::adapter::SyncableStore;
use truffle_core::store_sync::types::DeviceSlice;

pub const CLIPBOARD_STORE_ID: &str = "clipboard";

/// Internal state for the clipboard store.
struct StoreInner {
    device_id: String,
    /// Our local clipboard text + fingerprint
    local_text: Option<String>,
    local_fingerprint: u64,
    local_version: u64,
    local_updated_at: u64,
    /// Remote clipboard states keyed by device_id
    remote_slices: HashMap<String, RemoteClipboard>,
    /// Callback to notify StoreSyncAdapter of local changes
    on_change_tx: Option<mpsc::UnboundedSender<()>>,
}

struct RemoteClipboard {
    text: String,
    fingerprint: u64,
    version: u64,
    updated_at: u64,
}

pub struct ClipboardHistoryStore {
    inner: RwLock<StoreInner>,
}

impl ClipboardHistoryStore {
    pub fn new(device_id: String) -> Self;

    /// Set the change notification channel (connected to clipboard monitor).
    pub fn set_change_notifier(&self, tx: mpsc::UnboundedSender<()>);

    /// Called by clipboard monitor when local clipboard changes.
    /// Returns true if this is a genuinely new clip (not echo).
    pub fn update_local(&self, text: &str) -> bool;

    /// Called by clipboard monitor to get latest remote text to write to clipboard.
    /// Returns (text, fingerprint) of most recent remote clip, if newer than our last write.
    pub fn latest_remote(&self) -> Option<(String, u64)>;

    /// Get local fingerprint (used by echo guard).
    pub fn local_fingerprint(&self) -> u64;
}

impl SyncableStore for ClipboardHistoryStore {
    fn store_id(&self) -> &str { CLIPBOARD_STORE_ID }

    fn get_local_slice(&self) -> Option<DeviceSlice> {
        // Read lock on inner
        // Return DeviceSlice { device_id, data: json!({"text": ..., "fingerprint": ...}), version, updated_at }
    }

    fn apply_remote_slice(&self, slice: DeviceSlice) {
        // Extract text + fingerprint from slice.data
        // Write lock on inner, update remote_slices map
    }

    fn remove_remote_slice(&self, device_id: &str, _reason: &str) {
        // Write lock, remove from remote_slices
    }

    fn clear_remote_slices(&self) {
        // Write lock, clear remote_slices map
    }
}
```

### File: `src-tauri/src/clipboard/thread.rs`

**Purpose**: Dedicated OS thread that owns the `arboard::Clipboard` handle.

```rust
/// Commands sent to the clipboard thread.
pub enum ClipboardCommand {
    /// Read current clipboard text; reply on oneshot.
    Read(oneshot::Sender<Option<String>>),
    /// Write text to clipboard.
    Write(String),
    /// Shut down the thread.
    Shutdown,
}

/// Spawn a dedicated std::thread for arboard clipboard access.
/// Returns an mpsc::Sender<ClipboardCommand> for sending commands.
///
/// arboard MUST be used from a single OS thread because:
/// - On macOS it requires the NSPasteboard API from the main thread or a dedicated thread
/// - arboard::Clipboard is not Send
pub fn spawn_clipboard_thread() -> std::sync::mpsc::Sender<ClipboardCommand>;
```

Implementation detail: inside the thread, use `std::sync::mpsc::Receiver` and `arboard::Clipboard::new()`. The thread loops on `recv()`, handling Read/Write/Shutdown. For Read, create `arboard::Clipboard` (or reuse), call `get_text()`, send result back. For Write, call `set_text()`.

### File: `src-tauri/src/clipboard/monitor.rs`

**Purpose**: Polling loop that bridges OS clipboard and ClipboardHistoryStore.

```rust
/// Spawn the clipboard monitoring task on a tokio runtime.
/// Polls every 250ms, detects changes via xxh3 fingerprint, prevents echo loops.
///
/// - clipboard_cmd_tx: send commands to the clipboard OS thread
/// - store: the ClipboardHistoryStore (shared)
/// - adapter: the StoreSyncAdapter (to notify of local changes)
/// - shutdown: Notify to stop the loop
pub fn spawn_clipboard_monitor(
    clipboard_cmd_tx: std::sync::mpsc::Sender<ClipboardCommand>,
    store: Arc<ClipboardHistoryStore>,
    adapter: Arc<StoreSyncAdapter>,
    shutdown: Arc<tokio::sync::Notify>,
) -> tokio::task::JoinHandle<()>;
```

Inside the monitor loop (every 250ms):

1. **Read local clipboard**: Send `ClipboardCommand::Read` to the OS thread, await the oneshot response.
2. **Compute xxh3 fingerprint** of the text.
3. **Echo guard**: If fingerprint matches `store.local_fingerprint()`, skip (we wrote this ourselves).
4. **Dedup**: If fingerprint matches the last-known local fingerprint in the store, skip (no change).
5. **New local clip**: Call `store.update_local(text)`. If it returns true, get the local slice and call `adapter.handle_local_changed(CLIPBOARD_STORE_ID, &slice)`.
6. **Check for remote writes**: Call `store.latest_remote()`. If there is a newer remote clip whose fingerprint differs from the last thing we wrote to clipboard, send `ClipboardCommand::Write(text)` to the OS thread, and update the echo guard fingerprint in the store so next poll doesn't treat it as a new local clip.

**Implementation note for bridging std/tokio channels**: The clipboard thread uses `std::sync::mpsc`. To await the reply from tokio, use a `tokio::sync::oneshot` and a small bridge:

```rust
// In the clipboard thread's Read handler, instead of std::sync::mpsc oneshot,
// use a simple callback pattern:
// Actually, simpler approach: use crossbeam-channel or just tokio::task::spawn_blocking.

// REVISED: Use tokio::task::spawn_blocking to call the std::sync::mpsc synchronously:
let text = tokio::task::spawn_blocking(move || {
    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
    clipboard_cmd_tx.send(ClipboardCommand::Read(reply_tx)).ok()?;
    reply_rx.recv().ok().flatten()
}).await.unwrap_or(None);
```

Wait -- `ClipboardCommand::Read` uses `std::sync::mpsc::Sender` for the oneshot. Change the `Read` variant to:
```rust
Read(std::sync::mpsc::Sender<Option<String>>),
```
This way both sides use std channels. The monitor wraps its polling in `spawn_blocking` for the synchronous parts.

**Verification**: Integration test (manual): Copy text on machine, verify store.update_local was called (add tracing). Harder to unit-test due to clipboard access; rely on Step 2 and Step 3 unit tests for correctness, use tracing for integration.

**Dependencies**: Steps 2, 3.

### File: `src-tauri/src/mesh.rs`

**Purpose**: MeshNode initialization and event wiring.

```rust
/// Create and configure the MeshNode + StoreSyncAdapter + wire them together.
///
/// Returns the running mesh infrastructure.
pub struct MeshStack {
    pub mesh_node: Arc<MeshNode>,
    pub store_sync_adapter: Arc<StoreSyncAdapter>,
    pub outgoing_rx: mpsc::UnboundedReceiver<OutgoingSyncMessage>, // passed to wire_store_sync
    pub mesh_event_rx: broadcast::Receiver<MeshNodeEvent>,
}

pub fn create_mesh_stack(
    config: &AppConfig,
    connection_manager: Arc<ConnectionManager>,
    clipboard_store: Arc<ClipboardHistoryStore>,
) -> MeshStack {
    // 1. MeshNode::new(MeshNodeConfig { ... }, connection_manager)
    //    - device_id: config.device_id
    //    - device_name: config.device_name
    //    - device_type: "desktop"
    //    - hostname_prefix: "cheese"
    //    - prefer_primary: false
    //    - capabilities: vec!["clipboard-sync"]
    //    - metadata: None
    //    - timing: MeshTimingConfig::default()

    // 2. Create outgoing channel for StoreSyncAdapter
    //    let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();

    // 3. StoreSyncAdapter::new(
    //       StoreSyncAdapterConfig { local_device_id: config.device_id.clone() },
    //       vec![clipboard_store as Arc<dyn SyncableStore>],
    //       outgoing_tx,
    //    )

    // 4. Return MeshStack (caller will wire_store_sync after MeshNode::start())
}

/// Wire everything and start. Call this after sidecar is ready.
pub async fn start_mesh(
    mesh_node: &Arc<MeshNode>,
    adapter: &Arc<StoreSyncAdapter>,
    outgoing_rx: mpsc::UnboundedReceiver<OutgoingSyncMessage>,
) -> (JoinHandle<()>, JoinHandle<()>) {
    mesh_node.start().await;
    adapter.start().await;
    truffle_core::integration::wire_store_sync(mesh_node, adapter, outgoing_rx)
}
```

### File: `src-tauri/src/config.rs`

**Purpose**: Persistent app configuration.

```rust
use directories::ProjectDirs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub device_id: String,
    pub device_name: String,
}

impl AppConfig {
    /// Config file path: ~/Library/Application Support/com.cheeseboard.app/config.json
    pub fn config_path() -> PathBuf;

    /// State dir for tsnet: ~/Library/Application Support/com.cheeseboard.app/tsnet-state/
    pub fn state_dir() -> PathBuf;
}

/// Load existing config or create a new one with generated device_id.
pub fn load_or_create() -> AppConfig {
    // 1. Try read config_path()
    // 2. If exists, parse JSON
    // 3. If not, generate:
    //    - device_id: uuid::Uuid::new_v4().to_string()[..8] (short, readable)
    //    - device_name: hostname::get() or "Desktop"
    // 4. Write to disk, return
}
```

Use `directories::ProjectDirs::from("com", "cheeseboard", "app")` to get platform-appropriate paths. On macOS this resolves to `~/Library/Application Support/com.cheeseboard.app/`.

### File: `src-tauri/src/tray.rs`

**Purpose**: System tray icon and context menu.

```rust
use tauri::tray::{TrayIconBuilder, TrayIcon, MouseButton, MouseButtonState};
use tauri::menu::{Menu, MenuItem, Submenu};

/// Build the initial tray icon and menu.
pub fn build_tray(app: &tauri::App) -> Result<TrayIcon, tauri::Error> {
    let menu = build_menu(app, &[])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("Cheeseboard - Clipboard Sync")
        .on_menu_event(handle_menu_event)
        .build(app)
}

/// Build the tray context menu with the current device list.
fn build_menu(
    app: &impl tauri::Manager,
    devices: &[DeviceInfo],
) -> Result<Menu<tauri::Wry>, tauri::Error> {
    // Structure:
    // -----------------
    // Cheeseboard v0.1.0   (disabled title item)
    // -----------------
    // Devices:
    //   MacBook Pro - connected     (device entries, disabled)
    //   iMac - connected
    //   (none)                      (if no peers)
    // -----------------
    // Status: Connected             (or "Connecting...", "Disconnected")
    // -----------------
    // Quit                          (quit action)
}

/// Handle tray menu item clicks.
fn handle_menu_event(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Update the tray menu with a new device list. Called when MeshNode emits
/// DeviceDiscovered/DeviceOffline/DevicesChanged events.
pub fn update_device_list(
    tray: &TrayIcon,
    app: &impl tauri::Manager,
    devices: &[DeviceInfo],
);

/// Update the tray icon based on connection state.
pub fn set_tray_connected(tray: &TrayIcon, connected: bool);

/// Simple device info for the tray UI.
pub struct DeviceInfo {
    pub name: String,
    pub status: String, // "connected" | "offline"
}
```

---

## 3. Implementation Steps (in dependency order)

### Step 1: Project scaffold + config

**Files**: All scaffold files listed in Section 1, plus `config.rs`.

**What to do**:
1. Create directory structure.
2. Write `Cargo.toml`, `tauri.conf.json`, `build.rs`, `capabilities/default.json`, `src/index.html`.
3. Implement `config.rs` (load_or_create, config_path, state_dir).
4. Create minimal `main.rs` that initializes tracing, loads config, and starts Tauri with an empty `.setup()`.

**Verification**: `cd src-tauri && cargo build` compiles. Running the app shows a tray icon (use a placeholder icon).

**Dependencies**: None.

### Step 2: Clipboard OS thread

**Files**: `clipboard/mod.rs`, `clipboard/thread.rs`.

**What to do**:
1. Implement `ClipboardCommand` enum with `Read`, `Write`, `Shutdown` variants.
2. Implement `spawn_clipboard_thread()`:
   - Spawn `std::thread::spawn`
   - Inside: `let mut clipboard = arboard::Clipboard::new().unwrap();`
   - Loop on `std::sync::mpsc::Receiver<ClipboardCommand>::recv()`
   - `Read(tx)` -> `clipboard.get_text().ok()` -> `tx.send(result)`
   - `Write(text)` -> `clipboard.set_text(text)`
   - `Shutdown` -> break

**Verification**: Unit test that spawns the thread, sends Write("hello"), then sends Read and asserts it gets "hello" back.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_clipboard() {
        let tx = spawn_clipboard_thread();
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        tx.send(ClipboardCommand::Write("cheeseboard-test".into())).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        tx.send(ClipboardCommand::Read(reply_tx)).unwrap();
        let result = reply_rx.recv().unwrap();
        assert_eq!(result, Some("cheeseboard-test".to_string()));
        tx.send(ClipboardCommand::Shutdown).unwrap();
    }
}
```

**Dependencies**: Step 1.

### Step 3: ClipboardHistoryStore

**Files**: `clipboard/store.rs`.

**What to do**:
1. Implement `ClipboardHistoryStore` with `std::sync::RwLock<StoreInner>`.
2. Implement `SyncableStore` trait methods:
   - `store_id()` -> `"clipboard"`
   - `get_local_slice()` -> Build `DeviceSlice` with `data: json!({"text": text, "fingerprint": fp})`, version, updated_at.
   - `apply_remote_slice()` -> Parse `text` and `fingerprint` from `slice.data`, store in `remote_slices` map.
   - `remove_remote_slice()` -> Remove entry from map.
   - `clear_remote_slices()` -> Clear map.
3. Implement `update_local(text)`:
   - Compute `xxhash_rust::xxh3::xxh3_64(text.as_bytes())`.
   - If fingerprint unchanged, return false.
   - Otherwise update local_text, local_fingerprint, increment local_version, set updated_at to current millis.
   - Return true.
4. Implement `latest_remote()`:
   - Iterate remote_slices, find the one with highest updated_at.
   - Return `Some((text, fingerprint))`.

**Verification**: Unit tests:
- Create store, call `update_local("hello")`, assert `get_local_slice()` returns slice with `"hello"` in data.
- Call `apply_remote_slice(...)`, then `latest_remote()` returns the remote text.
- Call `remove_remote_slice(...)`, `latest_remote()` returns None.
- Call `update_local("hello")` twice, second call returns false (dedup).

**Dependencies**: Step 1.

### Step 4: Clipboard monitor

**Files**: `clipboard/monitor.rs`.

**What to do**:
1. Implement `spawn_clipboard_monitor()`.
2. The monitor runs on tokio (but clipboard reads/writes go through the OS thread channel).
3. Main loop uses `tokio::time::interval(Duration::from_millis(250))`.
4. Each tick:
   a. Read clipboard from OS thread (send Read command, await reply via a `tokio::sync::oneshot` bridged through std channel -- see implementation note below).
   b. If text is None or empty, skip.
   c. Compute fingerprint. Compare with last-known-written fingerprint (echo guard). If match, skip.
   d. Call `store.update_local(&text)`. If true (new clip), get local slice and call `adapter.handle_local_changed()`.
   e. Call `store.latest_remote()`. If there's a remote clip whose fingerprint differs from the last thing we wrote to OS clipboard, write it via the OS thread and update the echo guard.

**Implementation note for bridging std/tokio channels**: The clipboard thread uses `std::sync::mpsc`. To await the reply from tokio, use a `tokio::sync::oneshot` and a small bridge:

```rust
// REVISED: Use tokio::task::spawn_blocking to call the std::sync::mpsc synchronously:
let text = tokio::task::spawn_blocking(move || {
    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
    clipboard_cmd_tx.send(ClipboardCommand::Read(reply_tx)).ok()?;
    reply_rx.recv().ok().flatten()
}).await.unwrap_or(None);
```

**Verification**: Integration test (manual): Copy text on machine, verify store.update_local was called (add tracing). Harder to unit-test due to clipboard access; rely on Step 2 and Step 3 unit tests for correctness, use tracing for integration.

**Dependencies**: Steps 2, 3.

### Step 5: Sidecar stack initialization

**Files**: `sidecar.rs`.

**What to do**:
1. Implement `init_sidecar()`:
   ```rust
   pub async fn init_sidecar(config: &AppConfig) -> Result<SidecarStack, Box<dyn std::error::Error>> {
       // 1. Generate 32 random bytes for session token
       let session_token: [u8; 32] = rand::random();
       let session_token_hex = hex::encode(&session_token);

       // 2. Bind BridgeManager
       let mut bridge_manager = BridgeManager::bind(session_token).await?;
       let bridge_port = bridge_manager.local_port();

       // 3. Create ConnectionManager
       let transport_config = TransportConfig::default();
       let (connection_manager, transport_event_rx) = ConnectionManager::new(transport_config);
       let connection_manager = Arc::new(connection_manager);

       // 4. Register handlers on BridgeManager
       use truffle_core::bridge::header::Direction;
       use truffle_core::transport::connection::{WsIncomingHandler, WsOutgoingHandler};
       bridge_manager.add_handler(
           443, Direction::Incoming,
           Arc::new(WsIncomingHandler::new(connection_manager.clone())),
       );
       bridge_manager.add_handler(
           443, Direction::Outgoing,
           Arc::new(WsOutgoingHandler::new(connection_manager.clone())),
       );

       // 5. Spawn BridgeManager accept loop
       tokio::spawn(async move { bridge_manager.run().await });

       // 6. Resolve sidecar binary path
       let binary_path = resolve_sidecar_path();

       // 7. Spawn GoShim
       let shim_config = ShimConfig {
           binary_path,
           hostname: format!("cheese-desktop-{}", &config.device_id[..8]),
           state_dir: AppConfig::state_dir().to_string_lossy().to_string(),
           auth_key: None,
           bridge_port,
           session_token: session_token_hex,
           auto_restart: true,
       };
       let (go_shim, shim_lifecycle_rx) = GoShim::spawn(shim_config).await?;

       Ok(SidecarStack {
           bridge_manager_port: bridge_port,
           session_token,
           go_shim,
           shim_lifecycle_rx,
           connection_manager,
           transport_event_rx,
       })
   }

   fn resolve_sidecar_path() -> PathBuf {
       // In dev: ../binaries/sidecar-slim-aarch64-apple-darwin
       // In bundle: Tauri resolves via tauri::api::process::sidecar
       // For MVP, use env var or hardcoded path relative to binary
       let exe_dir = std::env::current_exe()
           .unwrap()
           .parent()
           .unwrap()
           .to_path_buf();
       exe_dir.join("sidecar-slim")
   }
   ```

2. Implement `spawn_lifecycle_listener()` that subscribes to `ShimLifecycleEvent`:
   - `AuthRequired { auth_url }` -> Call `open::that(&auth_url)` to open browser. Call `mesh_node.set_auth_required(&auth_url)`.
   - `Status(data)` where `data.state == "running"` and `!data.tailscale_ip.is_empty()` -> Call `mesh_node.set_auth_authenticated()`.
   - `Peers(data)` -> Convert peers to `truffle_core::types::TailnetPeer` vec, call `mesh_node.handle_tailnet_peers(&peers)`. Then for each online peer, initiate a dial via `go_shim.dial(peer.dns_name, 443)`, and when the dial completes (via BridgeManager), ConnectionManager handles it automatically.
   - `Crashed { .. }` -> Update tray icon to disconnected.
   - `Started` -> Update tray icon to connected.

**Verification**: `cargo build` succeeds. Manual test: run the app, check logs for "Go shim started" or auth URL opening in browser.

**Dependencies**: Steps 1, config.

### Step 6: Mesh stack wiring

**Files**: `mesh.rs`.

**What to do**:
1. Implement `create_mesh_stack()` as described in Section 2.
2. Implement `start_mesh()` as described in Section 2.
3. The key wiring sequence is:
   ```rust
   let mesh_stack = create_mesh_stack(&config, connection_manager, clipboard_store);
   let (h1, h2) = start_mesh(
       &mesh_stack.mesh_node,
       &mesh_stack.store_sync_adapter,
       mesh_stack.outgoing_rx,
   ).await;
   ```

**Verification**: Check logs for `[StoreSyncAdapter] Started with 1 stores` and `MeshNode starting with identity: ...`.

**Dependencies**: Steps 3, 5.

### Step 7: Tray UI

**Files**: `tray.rs`.

**What to do**:
1. Implement `build_tray()` as specified.
2. Implement `build_menu()` with the menu structure shown.
3. Implement `update_device_list()` -- rebuilds the menu with new device info.
4. Implement `set_tray_connected()` -- swaps tray icon between connected/disconnected.

For dynamic menu updates, spawn a task in `main.rs` that listens to `MeshNodeEvent` via `mesh_node.subscribe_events()`:

```rust
tokio::spawn(async move {
    loop {
        match mesh_event_rx.recv().await {
            Ok(MeshNodeEvent::DeviceDiscovered(device)) |
            Ok(MeshNodeEvent::DeviceUpdated(device)) => {
                let devices = mesh_node.devices().await;
                let infos: Vec<DeviceInfo> = devices.iter()
                    .filter(|d| d.id != config.device_id)
                    .map(|d| DeviceInfo {
                        name: d.name.clone(),
                        status: format!("{:?}", d.status).to_lowercase(),
                    })
                    .collect();
                tray::update_device_list(&tray, &app_handle, &infos);
            }
            Ok(MeshNodeEvent::DeviceOffline(_)) => {
                // Same: refresh device list
            }
            Ok(MeshNodeEvent::Started) => {
                tray::set_tray_connected(&tray, true);
            }
            Ok(MeshNodeEvent::Stopped) => {
                tray::set_tray_connected(&tray, false);
            }
            _ => {}
        }
    }
});
```

**Verification**: Visual check -- tray icon appears, right-click shows menu with "Quit". After connecting to Tailscale, device list populates.

**Dependencies**: Steps 5, 6.

### Step 8: Full orchestration in main.rs

**Files**: `main.rs`.

**What to do**: Wire everything together in the Tauri `.setup()` callback.

```rust
fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("cheeseboard=debug,truffle_core=debug")
        .init();

    let config = config::load_or_create();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let config = config.clone();

            // Build tray with empty device list
            let tray = tray::build_tray(app)?;

            tauri::async_runtime::spawn(async move {
                // 1. Start sidecar stack
                let sidecar = match sidecar::init_sidecar(&config).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!("Failed to start sidecar: {e}");
                        return;
                    }
                };

                // 2. Create clipboard store + thread
                let clipboard_store = Arc::new(
                    ClipboardHistoryStore::new(config.device_id.clone())
                );
                let clipboard_cmd_tx = clipboard::thread::spawn_clipboard_thread();

                // 3. Create mesh stack
                let mesh_stack = mesh::create_mesh_stack(
                    &config,
                    sidecar.connection_manager.clone(),
                    clipboard_store.clone(),
                );

                // 4. Start mesh + wire store sync
                let (h1, h2) = mesh::start_mesh(
                    &mesh_stack.mesh_node,
                    &mesh_stack.store_sync_adapter,
                    mesh_stack.outgoing_rx,
                ).await;

                // 5. Start clipboard monitor
                let shutdown = Arc::new(tokio::sync::Notify::new());
                let monitor_handle = clipboard::monitor::spawn_clipboard_monitor(
                    clipboard_cmd_tx,
                    clipboard_store.clone(),
                    mesh_stack.store_sync_adapter.clone(),
                    shutdown.clone(),
                );

                // 6. Lifecycle listener (auth, peers, status -> tray updates)
                sidecar::spawn_lifecycle_listener(
                    mesh_stack.mesh_node.clone(),
                    sidecar.go_shim,
                    sidecar.shim_lifecycle_rx,
                    tray.clone(),
                );

                // 7. Tray device list updater
                // (spawn task listening to mesh_node.subscribe_events())
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building tauri app")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // Cleanup: stop mesh, kill sidecar, shutdown monitor
            }
        });
}
```

**Verification**: Full app builds and runs. Tray icon visible. Logs show sidecar starting, auth flow (if needed), mesh node starting.

**Dependencies**: All previous steps.

---

## 4. Go Sidecar Build

### 4.1 Build command for macOS arm64

```bash
cd /Users/jamesyong/Projects/project100/p008/truffle/packages/sidecar-slim
GOOS=darwin GOARCH=arm64 go build -o ../../cheeseboard_sidecar_build/sidecar-slim -ldflags="-s -w" .
```

The `-ldflags="-s -w"` strips debug info to reduce binary size (~30MB -> ~20MB).

### 4.2 Placement for Tauri bundling

Copy the built binary to Cheeseboard's binaries directory with the Tauri-required naming convention:

```bash
cp sidecar-slim /Users/jamesyong/Projects/project100/p008/cheeseboard/binaries/sidecar-slim-aarch64-apple-darwin
```

Tauri's `externalBin` resolves the platform triple suffix automatically. The binary name in `tauri.conf.json` is `sidecar-slim`, and Tauri appends `-aarch64-apple-darwin` at build time on macOS arm64.

### 4.3 Tailscale auth flow

The flow is fully automated:

1. GoShim starts tsnet. If no cached state in `state_dir`, tsnet emits an auth URL.
2. GoShim sends `tsnet:authRequired` event with the `auth_url` to Rust via stdout JSON.
3. Rust's lifecycle listener calls `open::that(&auth_url)` to open the default browser.
4. User completes Tailscale login in browser.
5. tsnet detects auth completion, emits `tsnet:status` with `state: "running"` and a `tailscaleIP`.
6. GoShim sends the status event to Rust.
7. Rust calls `mesh_node.set_auth_authenticated()`.
8. tsnet state is cached in `state_dir`. Next launch skips auth.

### 4.4 Dev workflow shortcut

For development, symlink the pre-built sidecar binary:

```bash
ln -s /Users/jamesyong/Projects/project100/p008/truffle/packages/sidecar-slim/bin/sidecar-slim \
      /Users/jamesyong/Projects/project100/p008/cheeseboard/binaries/sidecar-slim-aarch64-apple-darwin
```

---

## 5. Testing Plan

### 5.1 Unit tests for ClipboardHistoryStore

Location: `src-tauri/src/clipboard/store.rs` (`#[cfg(test)]` module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_local_returns_true_on_new_text() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        assert!(store.update_local("hello"));
    }

    #[test]
    fn update_local_returns_false_on_same_text() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        assert!(store.update_local("hello"));
        assert!(!store.update_local("hello"));
    }

    #[test]
    fn get_local_slice_reflects_update() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.update_local("test clipboard");
        let slice = store.get_local_slice().unwrap();
        assert_eq!(slice.device_id, "dev-1");
        assert_eq!(slice.data["text"].as_str().unwrap(), "test clipboard");
        assert_eq!(slice.version, 1);
    }

    #[test]
    fn apply_and_get_remote_slice() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        let slice = DeviceSlice {
            device_id: "dev-2".to_string(),
            data: serde_json::json!({"text": "remote text", "fingerprint": 12345}),
            version: 1,
            updated_at: 1000,
        };
        store.apply_remote_slice(slice);
        let (text, fp) = store.latest_remote().unwrap();
        assert_eq!(text, "remote text");
        assert_eq!(fp, 12345);
    }

    #[test]
    fn remove_remote_slice() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.apply_remote_slice(DeviceSlice {
            device_id: "dev-2".to_string(),
            data: serde_json::json!({"text": "x", "fingerprint": 1}),
            version: 1,
            updated_at: 1000,
        });
        store.remove_remote_slice("dev-2", "offline");
        assert!(store.latest_remote().is_none());
    }

    #[test]
    fn clear_remote_slices() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.apply_remote_slice(DeviceSlice {
            device_id: "dev-2".to_string(),
            data: serde_json::json!({"text": "a", "fingerprint": 1}),
            version: 1,
            updated_at: 1000,
        });
        store.apply_remote_slice(DeviceSlice {
            device_id: "dev-3".to_string(),
            data: serde_json::json!({"text": "b", "fingerprint": 2}),
            version: 1,
            updated_at: 2000,
        });
        store.clear_remote_slices();
        assert!(store.latest_remote().is_none());
    }

    #[test]
    fn implements_syncable_store_trait() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        let arc: Arc<dyn SyncableStore> = Arc::new(store);
        assert_eq!(arc.store_id(), "clipboard");
    }

    #[test]
    fn version_increments_on_each_update() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.update_local("first");
        let v1 = store.get_local_slice().unwrap().version;
        store.update_local("second");
        let v2 = store.get_local_slice().unwrap().version;
        assert_eq!(v2, v1 + 1);
    }
}
```

### 5.2 Unit tests for clipboard thread

Location: `src-tauri/src/clipboard/thread.rs`

As shown in Step 2 verification. Note: these tests actually access the system clipboard, so mark them `#[ignore]` in CI:

```rust
#[test]
#[ignore] // Requires system clipboard access
fn roundtrip_clipboard() { ... }
```

### 5.3 Integration test: local clipboard change -> outgoing sync message

Location: `src-tauri/src/clipboard/monitor.rs` (or a separate `tests/` file)

This test doesn't need real clipboard access; it mocks the clipboard thread:

```rust
#[tokio::test]
async fn local_change_triggers_sync_broadcast() {
    // 1. Create ClipboardHistoryStore
    let store = Arc::new(ClipboardHistoryStore::new("dev-1".to_string()));

    // 2. Create StoreSyncAdapter with outgoing channel
    let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel();
    let adapter = StoreSyncAdapter::new(
        StoreSyncAdapterConfig { local_device_id: "dev-1".to_string() },
        vec![store.clone() as Arc<dyn SyncableStore>],
        outgoing_tx,
    );
    adapter.start().await;

    // Drain start messages
    while outgoing_rx.try_recv().is_ok() {}

    // 3. Simulate what the monitor does on local change
    let changed = store.update_local("new clipboard text");
    assert!(changed);
    let slice = store.get_local_slice().unwrap();
    adapter.handle_local_changed("clipboard", &slice).await;

    // 4. Check outgoing message
    let msg = outgoing_rx.try_recv().unwrap();
    assert_eq!(msg.msg_type, "store:sync:update");
    assert_eq!(msg.payload["storeId"].as_str().unwrap(), "clipboard");
    assert_eq!(msg.payload["data"]["text"].as_str().unwrap(), "new clipboard text");
}
```

### 5.4 Manual end-to-end test: two machines

**Prerequisites**:
- Two macOS machines (or one + a VM) on the same Tailscale network.
- Cheeseboard built and running on both.

**Test procedure**:

1. Launch Cheeseboard on Machine A. Complete Tailscale auth if prompted. Verify tray icon shows "connected" state.
2. Launch Cheeseboard on Machine B. Complete Tailscale auth. Verify tray shows Machine A in device list.
3. On Machine A, copy text "hello from A" (Cmd+C in any app).
4. Wait 1-2 seconds for sync.
5. On Machine B, paste (Cmd+V). Verify it pastes "hello from A".
6. On Machine B, copy text "hello from B".
7. Wait 1-2 seconds.
8. On Machine A, paste. Verify it pastes "hello from B".
9. On Machine A, rapidly copy 5 different strings. Verify Machine B receives the last one (no echo loops, no missing clips).
10. Kill Cheeseboard on Machine B. Verify Machine A's tray updates to show Machine B as offline.
11. Restart Cheeseboard on Machine B. Verify it reconnects and syncs.

---

## 6. Config & Persistence

### 6.1 Device ID

- Generated once on first launch: `uuid::Uuid::new_v4().to_string()` (truncated to first 8 chars for readability in hostnames).
- Persisted at: `~/Library/Application Support/com.cheeseboard.app/config.json`.
- The full UUID is stored; only the first 8 chars are used in the Tailscale hostname.
- Never regenerated unless config file is deleted.

### 6.2 App preferences

For MVP v0.1, the only persistent config is `device_id` and `device_name`. Stored as:

```json
{
  "device_id": "a1b2c3d4",
  "device_name": "James's MacBook Pro"
}
```

Future versions can add `sync_enabled`, `max_clipboard_size`, etc. to this file.

### 6.3 Tsnet state directory

- Path: `~/Library/Application Support/com.cheeseboard.app/tsnet-state/`
- This is passed to `GoShim` as `state_dir`. tsnet stores its Tailscale auth tokens and node keys here.
- Deleting this directory forces re-authentication.

### 6.4 Platform paths via `directories` crate

```rust
use directories::ProjectDirs;

fn project_dirs() -> ProjectDirs {
    ProjectDirs::from("com", "cheeseboard", "app")
        .expect("could not determine project directories")
}

// config_path -> project_dirs().config_dir() / "config.json"
// state_dir   -> project_dirs().data_dir() / "tsnet-state"
```

On macOS:
- `config_dir` = `~/Library/Application Support/com.cheeseboard.app/`
- `data_dir` = `~/Library/Application Support/com.cheeseboard.app/`

---

## 7. Tray UI

### 7.1 Menu definition

The tray menu is built using Tauri v2's `tauri::menu` module:

```rust
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};

fn build_menu(app: &impl tauri::Manager, devices: &[DeviceInfo]) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let mut builder = MenuBuilder::new(app);

    // Title
    builder = builder.item(
        &MenuItemBuilder::with_id("title", "Cheeseboard v0.1.0")
            .enabled(false)
            .build(app)?
    );
    builder = builder.separator();

    // Devices section
    if devices.is_empty() {
        builder = builder.item(
            &MenuItemBuilder::with_id("no-devices", "No devices connected")
                .enabled(false)
                .build(app)?
        );
    } else {
        for (i, device) in devices.iter().enumerate() {
            let label = format!("{} - {}", device.name, device.status);
            builder = builder.item(
                &MenuItemBuilder::with_id(format!("device-{i}"), label)
                    .enabled(false)
                    .build(app)?
            );
        }
    }

    builder = builder.separator();

    // Quit
    builder = builder.item(
        &MenuItemBuilder::with_id("quit", "Quit Cheeseboard")
            .accelerator("CmdOrCtrl+Q")
            .build(app)?
    );

    builder.build()
}
```

### 7.2 Dynamic menu updates

When the device list changes, rebuild the entire menu and set it on the tray:

```rust
pub fn update_device_list(
    tray: &TrayIcon,
    app: &impl tauri::Manager,
    devices: &[DeviceInfo],
) {
    if let Ok(menu) = build_menu(app, devices) {
        let _ = tray.set_menu(Some(menu));
    }
}
```

Tauri v2's tray API allows replacing the entire menu at any time. This is called whenever `MeshNodeEvent::DeviceDiscovered`, `DeviceOffline`, or `DevicesChanged` fires.

### 7.3 Tray icon states

Two states, two icons:

| State | Icon file | Description |
|-------|-----------|-------------|
| Connected | `tray-connected.png` | Solid clipboard icon |
| Disconnected | `tray-disconnected.png` | Faded/outline clipboard icon |

Both are 32x32 PNG template images (macOS renders them as monochrome menu bar icons).

```rust
pub fn set_tray_connected(tray: &TrayIcon, connected: bool) {
    let icon_path = if connected {
        include_bytes!("../icons/tray-connected.png")
    } else {
        include_bytes!("../icons/tray-disconnected.png")
    };
    if let Ok(icon) = tauri::image::Image::from_bytes(icon_path) {
        let _ = tray.set_icon(Some(icon));
    }
}
```

---

## 8. Error Handling

### 8.1 Sidecar crash recovery

`GoShim` already has built-in auto-restart with exponential backoff (`ShimConfig::auto_restart: true`). The implementation in `/Users/jamesyong/Projects/project100/p008/truffle/crates/truffle-core/src/bridge/shim.rs` handles:

- Initial restart delay: 1 second, doubling up to 30 seconds max.
- Auth storm prevention: If the last event was `AuthRequired` before crash, auto-restart is paused to avoid infinite auth loops. Resume via `go_shim.resume_auto_restart()`.
- All pending dials are drained (senders dropped) on crash, so callers get `DialCancelled` errors.

Cheeseboard's lifecycle listener should:
- On `Crashed`: Set tray icon to disconnected. Log the error. Do NOT try manual restart (GoShim handles it).
- On `Started` (after restart): Set tray icon to connected. Re-request peers via `go_shim.get_peers()`.

### 8.2 Tailscale not running / not authenticated

- If tsnet can't find Tailscale: GoShim's `tsnet:error` event fires with a descriptive error. The lifecycle listener logs it and keeps the tray in disconnected state.
- If auth is needed: The browser opens automatically (Section 4.3). If the user closes the browser without authenticating, the shim will keep retrying (tsnet polls the auth URL). No action needed from Cheeseboard.
- If auth expires: GoShim emits `AuthRequired` again. Open browser again.

### 8.3 Clipboard access failure

The clipboard OS thread handles `arboard::Clipboard::new()` failures:

```rust
// In spawn_clipboard_thread()
std::thread::spawn(move || {
    let mut clipboard = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to initialize clipboard: {e}");
            // Keep thread alive but return None for all reads
            // Retry initialization every 5 seconds
            loop {
                match rx.recv() {
                    Ok(ClipboardCommand::Read(reply)) => { let _ = reply.send(None); }
                    Ok(ClipboardCommand::Shutdown) => return,
                    _ => {}
                }
            }
        }
    };
    // ... normal loop
});
```

Individual `get_text()` or `set_text()` failures are logged but don't crash the thread. The monitor loop handles `None` returns gracefully (skips that tick).

### 8.4 Network partition (mesh node disconnected)

When all WebSocket connections drop:
- `ConnectionManager` emits `TransportEvent::Disconnected` for each connection.
- `MeshNode` processes these and emits `MeshNodeEvent::DeviceOffline` for each peer.
- `StoreSyncAdapter` calls `handle_device_offline()` which clears remote slices via `remove_remote_slice()`.
- The tray device list updates to show no connected devices.
- The clipboard store's `remote_slices` map empties, so no stale remote clips will be written.

When the network recovers:
- GoShim re-discovers peers (tsnet periodically polls).
- Dials are re-established, WebSocket connections come back up.
- `MeshNode` emits `DeviceDiscovered`, `StoreSyncAdapter` runs `handle_device_discovered()` which broadcasts our current state and requests theirs.
- Clipboard state re-syncs immediately.

No manual intervention is needed. The entire stack is designed for automatic recovery.

---

## Summary of Implementation Order

| Step | Files | Estimated Time | Depends On |
|------|-------|---------------|------------|
| 1 | Scaffold, config.rs | 20 min | -- |
| 2 | clipboard/thread.rs | 15 min | 1 |
| 3 | clipboard/store.rs | 30 min | 1 |
| 4 | clipboard/monitor.rs | 30 min | 2, 3 |
| 5 | sidecar.rs | 30 min | 1 |
| 6 | mesh.rs | 20 min | 3, 5 |
| 7 | tray.rs | 20 min | 5, 6 |
| 8 | main.rs (full wiring) | 30 min | all |
| 9 | Sidecar binary build | 5 min | -- (parallel) |
| 10 | Icons (2 PNGs) | 5 min | -- (parallel) |
| 11 | Unit tests | 20 min | 2, 3 |
| 12 | Integration test | 15 min | 3, 6 |
| 13 | Manual e2e test | 20 min | all |

**Total estimated time: ~4 hours** for a competent Rust developer familiar with the truffle-core APIs.

Critical path: Steps 1 -> 3 -> 6 -> 8. Steps 2, 4, 5, 7 can be parallelized where dependencies allow. The sidecar binary build (Step 9) and icon creation (Step 10) are fully independent and should be done first or in parallel.
