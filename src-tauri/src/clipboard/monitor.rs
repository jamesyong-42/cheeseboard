use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use truffle::network::NetworkProvider;
use truffle::Node;

use super::store::{ClipboardHistoryStore, CLIPBOARD_NAMESPACE};
use super::thread::ClipboardThread;

/// Poll interval for checking clipboard changes.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Payload sent over the mesh for clipboard sync.
#[derive(serde::Serialize, serde::Deserialize)]
struct ClipboardPayload {
    text: String,
    fingerprint: u64,
    device_id: String,
    device_name: String,
    timestamp: u64,
}

/// Clipboard monitor that polls the OS clipboard for changes
/// and writes remote clipboard entries to the OS clipboard.
pub struct ClipboardMonitor<N: NetworkProvider + 'static> {
    clipboard: ClipboardThread,
    store: Arc<ClipboardHistoryStore>,
    node: Arc<Node<N>>,
    device_id: String,
    device_name: String,
    /// Echo guard: fingerprint of the last text we wrote TO the clipboard.
    echo_guard_fp: Arc<AtomicU64>,
}

impl<N: NetworkProvider + 'static> ClipboardMonitor<N> {
    pub fn new(
        clipboard: ClipboardThread,
        store: Arc<ClipboardHistoryStore>,
        node: Arc<Node<N>>,
        device_id: String,
        device_name: String,
    ) -> Self {
        Self {
            clipboard,
            store,
            node,
            device_id,
            device_name,
            echo_guard_fp: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Run the monitor loop. This runs until the task is cancelled.
    pub async fn run(self) {
        tracing::info!("Clipboard monitor started (poll interval: {POLL_INTERVAL:?})");

        let mut poll_interval = tokio::time::interval(POLL_INTERVAL);
        poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut clip_rx = self.node.subscribe(CLIPBOARD_NAMESPACE);
        let mut peer_rx = self.node.on_peer_change();
        let mut last_local_fp: u64 = 0;

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    self.poll_local_clipboard(&mut last_local_fp).await;
                }
                result = clip_rx.recv() => {
                    match result {
                        Ok(msg) => self.handle_remote_message(msg).await,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("Clipboard receiver lagged by {n} messages");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!("Clipboard subscription closed, stopping monitor");
                            break;
                        }
                    }
                }
                result = peer_rx.recv() => {
                    match result {
                        Ok(truffle::session::PeerEvent::Joined(state)) => {
                            // Send current clipboard to new peer to establish WS connection.
                            // broadcast() only reaches already-connected peers, so we must
                            // use send() here to trigger the lazy WebSocket handshake.
                            self.send_current_to_peer(&state.id).await;
                        }
                        Ok(truffle::session::PeerEvent::Left(id)) => {
                            self.store.remove_remote(&id);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Check if the local clipboard has changed and broadcast the update.
    async fn poll_local_clipboard(&self, last_fp: &mut u64) {
        let text = match self.clipboard.read() {
            Some(t) if !t.is_empty() => t,
            _ => return,
        };

        let fp = ClipboardHistoryStore::fingerprint(&text);

        // Skip if same as last poll
        if fp == *last_fp {
            return;
        }
        *last_fp = fp;

        // Echo guard: skip if this is something we just wrote
        let guard_fp = self.echo_guard_fp.load(Ordering::Relaxed);
        if fp == guard_fp {
            return;
        }

        // Genuine local clipboard change
        if self.store.update_local(&text) {
            tracing::debug!("Local clipboard changed (fp={fp:#x}), broadcasting");

            if let Some(entry) = self.store.get_local_entry() {
                let payload = ClipboardPayload {
                    text: entry.text,
                    fingerprint: entry.fingerprint,
                    device_id: self.device_id.clone(),
                    device_name: self.device_name.clone(),
                    timestamp: entry.timestamp,
                };

                if let Ok(json) = serde_json::to_vec(&payload) {
                    self.node.broadcast(CLIPBOARD_NAMESPACE, &json).await;
                }
            }
        }
    }

    /// Send current clipboard to a specific peer (triggers WS connection).
    async fn send_current_to_peer(&self, peer_id: &str) {
        if let Some(entry) = self.store.get_local_entry() {
            let payload = ClipboardPayload {
                text: entry.text,
                fingerprint: entry.fingerprint,
                device_id: self.device_id.clone(),
                device_name: self.device_name.clone(),
                timestamp: entry.timestamp,
            };

            if let Ok(json) = serde_json::to_vec(&payload) {
                tracing::info!("Sending clipboard to new peer {peer_id} (establishing connection)");
                if let Err(e) = self.node.send(peer_id, CLIPBOARD_NAMESPACE, &json).await {
                    tracing::debug!("Failed to send to peer {peer_id}: {e}");
                }
            }
        } else {
            // No clipboard content yet, but still trigger connection by sending empty ping
            tracing::info!("New peer {peer_id} discovered, establishing connection");
            let ping = serde_json::to_vec(&serde_json::json!({
                "text": "",
                "fingerprint": 0u64,
                "device_id": &self.device_id,
                "device_name": &self.device_name,
                "timestamp": 0u64,
            }))
            .unwrap();
            let _ = self.node.send(peer_id, CLIPBOARD_NAMESPACE, &ping).await;
        }
    }

    /// Handle an incoming clipboard message from a remote device.
    async fn handle_remote_message(&self, msg: truffle::NamespacedMessage) {
        let payload: ClipboardPayload = match serde_json::from_value(msg.payload) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("Failed to parse clipboard message: {e}");
                return;
            }
        };

        // Skip our own messages
        if payload.device_id == self.device_id {
            return;
        }

        // Skip empty pings used for connection establishment
        if payload.text.is_empty() {
            return;
        }

        self.store
            .apply_remote(&payload.device_id, &payload.text, payload.fingerprint, payload.timestamp);
        self.apply_latest_remote();
    }

    /// Write the latest remote clipboard entry to the OS clipboard.
    fn apply_latest_remote(&self) {
        if let Some((text, _ts)) = self.store.latest_remote() {
            let fp = ClipboardHistoryStore::fingerprint(&text);

            // Check if our local clipboard already has this content
            if let Some(local_fp) = self.store.local_fingerprint() {
                if local_fp == fp {
                    return;
                }
            }

            tracing::debug!("Writing remote clipboard to OS (fp={fp:#x})");

            // Set echo guard BEFORE writing so the next poll skips it
            self.echo_guard_fp.store(fp, Ordering::Relaxed);
            self.clipboard.write(text);
        }
    }
}
