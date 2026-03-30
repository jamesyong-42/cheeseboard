# Cheeseboard

[![CI](https://github.com/jamesyong-42/cheeseboard/actions/workflows/ci.yml/badge.svg)](https://github.com/jamesyong-42/cheeseboard/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/jamesyong-42/cheeseboard)](https://github.com/jamesyong-42/cheeseboard/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Cross-device clipboard sync, built on [truffle](https://github.com/jamesyong-42/truffle) mesh networking and Tailscale.

Copy text on one device, paste on another. No cloud servers, no accounts -- just your Tailscale network.

## Install

Download the latest release for your platform from [GitHub Releases](https://github.com/jamesyong-42/cheeseboard/releases):

| Platform | Download |
|----------|----------|
| macOS (Apple Silicon) | `.dmg` |
| macOS (Intel) | `.dmg` |
| Windows (x64) | `-setup.exe` (NSIS) |
| Linux (x64) | `.deb` or `.AppImage` |

Just download, install, and run. No dependencies required -- everything is bundled.

## First launch

1. Open Cheeseboard -- a tray icon appears and the onboarding window opens
2. Click **Sign in with Tailscale** -- authenticates via your browser
3. Once connected, the window shows your device list
4. Close the window -- Cheeseboard continues running in the system tray

On subsequent launches, Cheeseboard connects automatically with no window.

## How it works

Cheeseboard runs as a system tray app. Under the hood:

1. Polls your clipboard every 250ms for changes
2. Broadcasts new content to all connected peers via [truffle](https://github.com/jamesyong-42/truffle) mesh
3. Writes incoming remote clipboard content to your local clipboard
4. Uses xxh3 fingerprinting + echo guards to prevent sync loops
5. Cleans up remote state when peers disconnect

All traffic is end-to-end encrypted through Tailscale's WireGuard tunnels. The truffle sidecar (bundled with the app) manages its own Tailscale identity -- no Tailscale desktop app needed.

## Auto-update

Cheeseboard checks for updates on launch via signed update manifests on GitHub Releases. When a new version is available, you'll be prompted to update in-app.

## Build from source

### Prerequisites

- [Rust](https://rustup.rs/) 1.75+
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (platform-specific system deps)

### Steps

```bash
git clone https://github.com/jamesyong-42/cheeseboard.git
cd cheeseboard

# Generate placeholder icons (real icons replace these later)
cd src-tauri/icons && rustc gen_placeholder.rs -o gen_placeholder && ./gen_placeholder && cd ../..

# Run in development
cd src-tauri && cargo run

# Or build release installers
cd src-tauri && cargo tauri build
```

The `truffle` crate (from [crates.io](https://crates.io/crates/truffle)) automatically downloads the platform-specific sidecar binary at build time. No manual setup needed.

## Architecture

```
Cheeseboard (Tauri v2)
|
+-- Onboarding Window (first-launch Tailscale auth)
|
+-- System Tray (status, peer list, quit)
|     |
|     +-- PeerEvent stream (joined, left, connected, auth required)
|
+-- Clipboard Monitor (250ms poll + echo guard)
|     |
|     +-- ClipboardThread (dedicated OS thread for arboard)
|     +-- ClipboardHistoryStore (local + remote entries, xxh3 dedup)
|     +-- PeerEvent::Left -> remove stale remote clips
|
+-- truffle Node (mesh networking, all-in-one)
      |
      +-- broadcast("cheeseboard.clipboard", payload)
      +-- subscribe("cheeseboard.clipboard")
      +-- on_peer_change() -> tray + onboarding updates
      +-- Sidecar (Go/tsnet, bundled via truffle-sidecar crate)
```

### Sync protocol

Namespace: `cheeseboard.clipboard`

```json
{
  "text": "copied text",
  "fingerprint": 12345,
  "device_id": "uuid",
  "device_name": "MacBook",
  "timestamp": 1711612800000
}
```

### Key design choices

- **Dedicated OS thread** for clipboard access (macOS requires arboard on a single thread)
- **Echo guard** via `AtomicU64` fingerprint prevents write-back loops
- **Peer cleanup** removes stale remote clips when devices go offline
- **No visible window** after onboarding -- tray-only app

## Configuration

Config stored at platform-specific locations:

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/com.cheeseboard.Cheeseboard/config.json` |
| Linux | `~/.config/com.cheeseboard.Cheeseboard/config.json` |
| Windows | `%APPDATA%\com.cheeseboard.Cheeseboard\config.json` |

Contains: `device_id` (UUID, auto-generated) and `device_name` (hostname).

## Release process

Tag-triggered via GitHub Actions using [tauri-apps/tauri-action](https://github.com/tauri-apps/tauri-action):

```bash
# Bump version in Cargo.toml + tauri.conf.json
git commit -am "release: v0.2.0"
git tag v0.2.0 && git push --tags
```

This builds all platforms in parallel and uploads installers to GitHub Releases with signed update manifests.

| Platform | Formats |
|----------|---------|
| macOS | `.dmg` + `.app.tar.gz` + `.sig` |
| Windows | NSIS `-setup.exe` + `.sig` |
| Linux | `.deb` + `.AppImage` |

## Tech stack

- [Tauri v2](https://v2.tauri.app/) -- desktop app framework
- [truffle](https://crates.io/crates/truffle) -- P2P mesh networking over Tailscale
- [arboard](https://crates.io/crates/arboard) -- cross-platform clipboard access
- [xxhash-rust](https://crates.io/crates/xxhash-rust) -- fast fingerprinting for dedup

## License

[MIT](LICENSE)
