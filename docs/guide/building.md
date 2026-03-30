# Building from Source

## Prerequisites

- [Rust](https://rustup.rs/) 1.75+
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (platform-specific system deps)
- [Node.js](https://nodejs.org/) 22+ (for `cargo tauri build` only)

### Linux system dependencies

```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  patchelf \
  libxdo-dev
```

## Development

```bash
git clone https://github.com/jamesyong-42/cheeseboard.git
cd cheeseboard

# Generate placeholder icons
cd src-tauri/icons
rustc gen_placeholder.rs -o gen_placeholder
./gen_placeholder
cd ../..

# Run in development mode
cd src-tauri
cargo run
```

The `truffle` crate automatically downloads the platform-specific sidecar binary from GitHub releases at build time. No manual setup needed.

::: tip
Set `TRUFFLE_SIDECAR_SKIP_DOWNLOAD=1` to skip the sidecar download (useful if you're offline or providing your own binary).
:::

## Build installers

```bash
cd src-tauri
cargo tauri build
```

This produces platform-specific installers in `target/release/bundle/`:

| Platform | Output |
|----------|--------|
| macOS | `bundle/dmg/*.dmg` |
| Windows | `bundle/nsis/*-setup.exe` |
| Linux | `bundle/deb/*.deb` + `bundle/appimage/*.AppImage` |

## Running tests

```bash
cd src-tauri
cargo test
```

## Linting

```bash
cd src-tauri
cargo fmt --check
cargo clippy -- -D warnings
```

## Release process

Releases are tag-triggered via GitHub Actions:

```bash
# 1. Bump version in Cargo.toml and tauri.conf.json
# 2. Commit
git commit -am "release: v0.2.0"

# 3. Tag and push
git tag v0.2.0
git push && git push --tags
```

GitHub Actions will:
1. Build for macOS (arm64 + x64), Linux (x64), Windows (x64)
2. Create a GitHub Release with all installers
3. Generate signed update manifests for the auto-updater

### Signing

Release builds are signed using Tauri's updater signature system. The signing key is stored in GitHub Secrets:

- `TAURI_SIGNING_PRIVATE_KEY` -- the private key
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` -- key password (empty for no password)

Generate a new key pair with:

```bash
cargo tauri signer generate -w ~/.tauri/cheeseboard.key
```
