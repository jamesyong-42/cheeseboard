# Getting Started

## Install

Download the latest release for your platform:

::: code-group
```sh [macOS]
# Download .dmg from GitHub Releases
open https://github.com/jamesyong-42/cheeseboard/releases/latest
```

```sh [Linux]
# Debian/Ubuntu (.deb)
sudo dpkg -i cheeseboard_*.deb

# Or use the AppImage
chmod +x Cheeseboard_*.AppImage
./Cheeseboard_*.AppImage
```

```powershell [Windows]
# Download and run the NSIS installer
# Cheeseboard_*_x64-setup.exe
```
:::

## First launch

1. **Open Cheeseboard** -- a tray icon appears and the onboarding window opens
2. **Sign in with Tailscale** -- click the button, authenticate in your browser
3. **Connected** -- the window shows your device list and status
4. **Close the window** -- Cheeseboard keeps running in the system tray

::: tip
You don't need the Tailscale desktop app installed. Cheeseboard bundles its own Tailscale integration via the truffle sidecar. You just need a Tailscale account.
:::

## Using it

Once running on two or more devices:

1. **Copy** text on Device A
2. Within a second, the text appears in Device B's clipboard
3. **Paste** on Device B -- done

Cheeseboard syncs in the background. The tray icon shows your connected devices.

## Subsequent launches

After the first sign-in, Cheeseboard remembers your authentication. It connects automatically on launch with no window -- just the tray icon.

## Auto-updates

Cheeseboard checks for updates on launch. When a new version is available, you'll be prompted to update in-app. Updates are signed for integrity verification.

## Uninstall

- **macOS**: Drag Cheeseboard to Trash
- **Windows**: Use Settings > Apps or the uninstaller
- **Linux**: `sudo dpkg -r cheeseboard` or delete the AppImage

Config is stored separately and is not removed on uninstall. To fully clean up, delete:

| Platform | Config path |
|----------|-------------|
| macOS | `~/Library/Application Support/com.cheeseboard.Cheeseboard/` |
| Linux | `~/.config/com.cheeseboard.Cheeseboard/` |
| Windows | `%APPDATA%\com.cheeseboard.Cheeseboard\` |
