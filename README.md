# Cheeseboard

[![CI](https://github.com/jamesyong-42/cheeseboard/actions/workflows/ci.yml/badge.svg)](https://github.com/jamesyong-42/cheeseboard/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/jamesyong-42/cheeseboard)](https://github.com/jamesyong-42/cheeseboard/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Cross-device clipboard sync, built on [truffle](https://github.com/jamesyong-42/truffle) mesh networking and Tailscale.

Copy text on one device, paste on another. No cloud servers, no accounts -- just your Tailscale network.

## How it works

Cheeseboard runs as a system tray app with no visible windows. It:

1. Polls your clipboard every 250ms for changes
2. Broadcasts new clipboard content to all connected peers via truffle's mesh network
3. Writes incoming remote clipboard content to your local clipboard
4. Uses xxh3 fingerprinting + echo guards to prevent sync loops

All traffic is end-to-end encrypted through Tailscale.

## Install

Download the latest release for your platform from [GitHub Releases](https://github.com/jamesyong-42/cheeseboard/releases):

| Platform | Download |
|----------|----------|
| macOS (Apple Silicon) | `Cheeseboard_x.x.x_aarch64.dmg` |
| macOS (Intel) | `Cheeseboard_x.x.x_x64.dmg` |
| Linux (x64) | `cheeseboard_x.x.x_amd64.deb` / `.AppImage` |
| Linux (ARM64) | `cheeseboard_x.x.x_arm64.deb` / `.AppImage` |
| Windows (x64) | `Cheeseboard_x.x.x_x64-setup.exe` / `.msi` |

### Prerequisites

- [Tailscale](https://tailscale.com/) installed and logged in on all devices
- Cheeseboard running on each device you want to sync

## Build from source

### Prerequisites

- [Rust](https://rustup.rs/) 1.75+
- [Go](https://go.dev/) 1.22+ (for building the truffle sidecar)
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/) prerequisites (platform-specific system deps)

### Steps

```bash
# Clone with truffle dependency
git clone https://github.com/jamesyong-42/cheeseboard.git
cd cheeseboard

# Build the truffle sidecar (from sibling truffle project)
# Place binary in binaries/ as sidecar-slim or truffle-sidecar

# Generate placeholder icons
cd src-tauri/icons && rustc gen_placeholder.rs -o gen_placeholder && ./gen_placeholder && cd ../..

# Build the app
cd src-tauri && cargo build --release
```

For development:

```bash
cd src-tauri && cargo run
```

## Architecture

```
Cheeseboard (Tauri v2, tray-only)
  |
  +-- Clipboard Monitor (250ms poll, echo guard)
  |     |
  |     +-- ClipboardThread (dedicated OS thread for arboard)
  |     +-- ClipboardHistoryStore (local + remote entries, xxh3 dedup)
  |
  +-- truffle Node (mesh networking)
  |     |
  |     +-- broadcast("cheeseboard.clipboard", payload)
  |     +-- subscribe("cheeseboard.clipboard")
  |     +-- on_peer_change() -> tray updates
  |
  +-- System Tray (status, peer list, quit)
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

## Configuration

Config stored at platform-specific locations:

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/com.cheeseboard.Cheeseboard/config.json` |
| Linux | `~/.config/com.cheeseboard.Cheeseboard/config.json` |
| Windows | `%APPDATA%\com.cheeseboard.Cheeseboard\config.json` |

Contains: `device_id` (UUID, auto-generated) and `device_name` (hostname).

## Release process

Releases are automated via [release-please](https://github.com/googleapis/release-please):

1. Merge commits with conventional prefixes (`feat:`, `fix:`) to `main`
2. Release-please opens a Release PR with version bump + changelog
3. Merge the Release PR to trigger cross-platform builds
4. Tauri builds produce installers for all 5 platforms
5. Installers uploaded to GitHub Releases automatically

## License

[MIT](LICENSE)
