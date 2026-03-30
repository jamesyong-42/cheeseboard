# How It Works

## Overview

Cheeseboard monitors your OS clipboard and syncs changes across all your devices via a peer-to-peer mesh network. No cloud servers are involved -- all traffic is end-to-end encrypted through Tailscale's WireGuard tunnels.

## Clipboard monitoring

A polling loop checks the OS clipboard every **250ms** for changes. When new text is detected:

1. Compute an **xxh3 fingerprint** of the text
2. Compare against the last known fingerprint (skip if unchanged)
3. Check the **echo guard** (skip if we just wrote this text from a remote device)
4. If genuinely new, broadcast to all connected peers

## Sync protocol

Clipboard content is broadcast over truffle's namespace messaging system:

- **Namespace**: `cheeseboard.clipboard`
- **Transport**: WebSocket over Tailscale tunnel
- **Delivery**: Broadcast to all connected peers

```json
{
  "text": "copied text",
  "fingerprint": 12345,
  "device_id": "a1b2c3d4-...",
  "device_name": "MacBook",
  "timestamp": 1711612800000
}
```

## Echo guard

Without protection, a sync loop would occur:

> Device A copies text -> broadcasts to B -> B writes to clipboard -> B detects "change" -> broadcasts back to A -> infinite loop

The **echo guard** prevents this. Before writing a remote clip to the OS clipboard, Cheeseboard stores the fingerprint in an `AtomicU64`. On the next poll, if the clipboard fingerprint matches the echo guard, it's skipped.

## Peer discovery

Cheeseboard uses truffle's `Node` API which handles:

- **Sidecar process**: A bundled Go binary that speaks the Tailscale protocol via `tsnet`
- **Peer discovery**: Watches the Tailscale network for other Cheeseboard instances
- **Connection management**: Lazy WebSocket connections, automatic reconnection with exponential backoff
- **Authentication**: First-run Tailscale OAuth flow via the onboarding window

When a peer goes offline (`PeerEvent::Left`), their stale clipboard data is cleaned up.

## Security

- All traffic encrypted via **WireGuard** (Tailscale)
- No data touches any server -- direct peer-to-peer
- Update packages are **signed** with a private key (Tauri updater)
- No clipboard history is persisted to disk -- only the latest entry per device is held in memory
