use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use xxhash_rust::xxh3::xxh3_64;

/// Namespace used for clipboard sync messages over truffle.
pub const CLIPBOARD_NAMESPACE: &str = "cheeseboard.clipboard";

/// A clipboard history entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClipboardEntry {
    /// The clipboard text content.
    pub text: String,
    /// xxh3 fingerprint of the text for dedup.
    pub fingerprint: u64,
    /// Timestamp when this entry was captured (ms since epoch).
    pub timestamp: u64,
}

/// A remote device's clipboard content.
#[derive(Debug, Clone)]
struct RemoteClip {
    text: String,
    fingerprint: u64,
    timestamp: u64,
}

/// Internal state protected by RwLock.
struct StoreInner {
    device_id: String,
    /// The latest local clipboard entry (our device's contribution).
    local_entry: Option<ClipboardEntry>,
    /// Remote device clips, keyed by device_id.
    remote_clips: HashMap<String, RemoteClip>,
}

/// Clipboard history store.
///
/// Uses `std::sync::RwLock` (NOT tokio) so it can be accessed from
/// both async and sync contexts without requiring `.await`.
pub struct ClipboardHistoryStore {
    inner: RwLock<StoreInner>,
}

impl ClipboardHistoryStore {
    pub fn new(device_id: String) -> Self {
        Self {
            inner: RwLock::new(StoreInner {
                device_id,
                local_entry: None,
                remote_clips: HashMap::new(),
            }),
        }
    }

    /// Compute the xxh3 fingerprint for a string.
    pub fn fingerprint(text: &str) -> u64 {
        xxh3_64(text.as_bytes())
    }

    /// Update the local clipboard entry. Returns `true` if the content
    /// actually changed (not a duplicate).
    pub fn update_local(&self, text: &str) -> bool {
        let fp = Self::fingerprint(text);

        let mut inner = self.inner.write().unwrap();

        // Dedup: skip if fingerprint matches current entry
        if let Some(ref entry) = inner.local_entry {
            if entry.fingerprint == fp {
                return false;
            }
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        inner.local_entry = Some(ClipboardEntry {
            text: text.to_string(),
            fingerprint: fp,
            timestamp: now_ms,
        });

        true
    }

    /// Get the current local clipboard entry (for broadcasting).
    pub fn get_local_entry(&self) -> Option<ClipboardEntry> {
        let inner = self.inner.read().unwrap();
        inner.local_entry.clone()
    }

    /// Get the local fingerprint (for echo guard).
    pub fn local_fingerprint(&self) -> Option<u64> {
        let inner = self.inner.read().unwrap();
        inner.local_entry.as_ref().map(|e| e.fingerprint)
    }

    /// Apply a remote clipboard update from another device.
    ///
    /// The fingerprint is always recomputed locally — caller-supplied
    /// values come from the network and cannot be trusted.
    pub fn apply_remote(&self, device_id: &str, text: &str, timestamp: u64) {
        let fp = Self::fingerprint(text);

        let mut inner = self.inner.write().unwrap();
        // Don't apply our own clips
        if device_id == inner.device_id {
            return;
        }
        // Dedup: skip if we already have this exact content from this device
        if let Some(existing) = inner.remote_clips.get(device_id) {
            if existing.fingerprint == fp {
                return;
            }
        }
        tracing::debug!("Applied remote clip from device {device_id}");
        inner.remote_clips.insert(
            device_id.to_string(),
            RemoteClip {
                text: text.to_string(),
                fingerprint: fp,
                timestamp,
            },
        );
    }

    /// Get the latest remote clipboard entry (most recent across all remote devices).
    pub fn latest_remote(&self) -> Option<(String, u64)> {
        let inner = self.inner.read().unwrap();

        inner
            .remote_clips
            .values()
            .max_by_key(|clip| clip.timestamp)
            .map(|clip| (clip.text.clone(), clip.timestamp))
    }

    /// Remove a remote device's clipboard data.
    pub fn remove_remote(&self, device_id: &str) {
        let mut inner = self.inner.write().unwrap();
        if inner.remote_clips.remove(device_id).is_some() {
            tracing::debug!("Removed remote clip for device {device_id}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_local_returns_true_on_new_content() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        assert!(store.update_local("hello world"));
    }

    #[test]
    fn update_local_returns_false_on_duplicate() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        assert!(store.update_local("hello"));
        assert!(!store.update_local("hello"));
    }

    #[test]
    fn update_local_returns_true_on_different_content() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        assert!(store.update_local("first"));
        assert!(store.update_local("second"));
    }

    #[test]
    fn fingerprint_consistency() {
        let fp1 = ClipboardHistoryStore::fingerprint("test");
        let fp2 = ClipboardHistoryStore::fingerprint("test");
        assert_eq!(fp1, fp2);

        let fp3 = ClipboardHistoryStore::fingerprint("different");
        assert_ne!(fp1, fp3);
    }

    #[test]
    fn get_local_entry_none_when_empty() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        assert!(store.get_local_entry().is_none());
    }

    #[test]
    fn get_local_entry_returns_data() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.update_local("clipboard content");
        let entry = store.get_local_entry().unwrap();
        assert_eq!(entry.text, "clipboard content");
        assert!(entry.timestamp > 0);
    }

    #[test]
    fn apply_remote_and_latest() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.apply_remote("dev-2", "remote clip", 2000);

        let (text, ts) = store.latest_remote().unwrap();
        assert_eq!(text, "remote clip");
        assert_eq!(ts, 2000);
    }

    #[test]
    fn latest_remote_picks_most_recent() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.apply_remote("dev-2", "older", 1000);
        store.apply_remote("dev-3", "newer", 3000);

        let (text, _) = store.latest_remote().unwrap();
        assert_eq!(text, "newer");
    }

    #[test]
    fn remove_remote_works() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.apply_remote("dev-2", "x", 1);
        store.remove_remote("dev-2");
        assert!(store.latest_remote().is_none());
    }

    #[test]
    fn apply_own_clip_is_ignored() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.apply_remote("dev-1", "self", 1);
        assert!(store.latest_remote().is_none());
    }

    #[test]
    fn apply_remote_dedups_same_text() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.apply_remote("dev-2", "hello", 1000);
        store.apply_remote("dev-2", "hello", 2000);

        // Same text should be dedup'd; original timestamp preserved
        let (text, ts) = store.latest_remote().unwrap();
        assert_eq!(text, "hello");
        assert_eq!(ts, 1000);
    }

    #[test]
    fn apply_remote_replaces_on_different_text() {
        let store = ClipboardHistoryStore::new("dev-1".to_string());
        store.apply_remote("dev-2", "hello", 1000);
        store.apply_remote("dev-2", "world", 2000);

        let (text, ts) = store.latest_remote().unwrap();
        assert_eq!(text, "world");
        assert_eq!(ts, 2000);
    }
}
