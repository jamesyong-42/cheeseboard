# Architecture

## Module structure

```
src-tauri/src/
  main.rs            Entry point, Tauri setup, Node construction
  config.rs          Device ID/name persistence
  tray.rs            System tray menu + peer event handler
  clipboard/
    mod.rs           Module exports
    thread.rs        Dedicated OS thread for arboard clipboard access
    store.rs         Local + remote clipboard state with xxh3 dedup
    monitor.rs       Polling loop + broadcast/subscribe + echo guard
```

## Component diagram

```
+---------------------------------------------------------------+
|  Cheeseboard (Tauri v2)                                       |
|                                                               |
|  +-- Onboarding Window ----+  +-- System Tray -------------+ |
|  |  Tailscale auth flow    |  |  Status, peer list, quit    | |
|  |  (shown on first run)   |  |  PeerEvent stream           | |
|  +-------------------------+  +-----------------------------+ |
|                                                               |
|  +-- Clipboard Monitor (tokio::select!) --------------------+ |
|  |                                                           | |
|  |  poll_interval.tick()  -->  poll_local_clipboard()        | |
|  |    |                          |                           | |
|  |    |  ClipboardThread         |  store.update_local()     | |
|  |    |  (std::thread + mpsc)    |  node.broadcast()         | |
|  |    |                          |                           | |
|  |  clip_rx.recv()  ---------->  handle_remote_message()     | |
|  |    |                          |                           | |
|  |    |  node.subscribe()        |  store.apply_remote()     | |
|  |    |                          |  apply_latest_remote()    | |
|  |    |                          |                           | |
|  |  peer_rx.recv()  ---------->  store.remove_remote()       | |
|  |    |                          (cleanup on peer left)      | |
|  +-----------------------------------------------------------+ |
|                                                               |
|  +-- truffle Node ------------------------------------------+ |
|  |  NodeBuilder::default()                                   | |
|  |    .name("cheeseboard-{id}")                              | |
|  |    .sidecar_path(truffle::sidecar_path())                 | |
|  |    .state_dir(...)                                        | |
|  |    .build().await                                         | |
|  |                                                           | |
|  |  Methods used:                                            | |
|  |    broadcast(namespace, data)  -- send to all peers       | |
|  |    subscribe(namespace)        -- receive messages         | |
|  |    on_peer_change()            -- peer lifecycle events   | |
|  +-----------------------------------------------------------+ |
|                                                               |
|  +-- Go Sidecar (bundled via truffle-sidecar crate) --------+ |
|  |  tsnet: Tailscale network access                          | |
|  |  WireGuard encrypted tunnels                              | |
|  |  Peer discovery via Tailscale coordination server         | |
|  +-----------------------------------------------------------+ |
+---------------------------------------------------------------+
```

## Key design decisions

### Dedicated clipboard thread

`arboard::Clipboard` must live on a single OS thread (especially on macOS/Wayland). We use `std::thread` + `std::sync::mpsc` instead of tokio, keeping clipboard access off the async runtime.

### Echo guard with AtomicU64

The echo guard stores the fingerprint of the last text we wrote to the clipboard. On the next poll, if the fingerprint matches, it's a write-back from our own sync -- skip it. Uses `AtomicU64` with `Relaxed` ordering for lock-free performance.

### std::sync::RwLock for the store

The clipboard store uses `std::sync::RwLock` (not `tokio::RwLock`) so it can be accessed from both async tasks and the sync clipboard thread without `.await`.

### Three-armed select loop

The clipboard monitor runs a `tokio::select!` with three arms:
1. **Poll timer** (250ms) -- check local clipboard
2. **Message subscription** -- incoming remote clips
3. **Peer events** -- cleanup when peers disconnect

## Dependencies

| Crate | Purpose |
|-------|---------|
| `truffle` | P2P mesh networking (includes `truffle-core` + `truffle-sidecar`) |
| `tauri` | Desktop app framework with tray icon support |
| `tauri-plugin-updater` | Signed in-app auto-updates |
| `arboard` | Cross-platform clipboard access |
| `xxhash-rust` | Fast xxh3 fingerprinting for dedup |
| `tokio` | Async runtime |
| `tracing` | Structured logging |
| `open` | Cross-platform URL opening (for auth flow) |
