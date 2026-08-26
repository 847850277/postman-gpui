# Installation

Postman GPUI v0.1 supports macOS, Windows, and Linux. Download artifacts from
[GitHub Releases](https://github.com/847850277/postman-gpui/releases).

Release candidates may be unsigned. Their release notes must state that explicitly; do not bypass
an operating-system security warning unless the artifact and its SHA-256 checksum came from this
repository's release page.

Inter and JetBrains Mono are embedded for consistent cross-platform text rendering. Inter includes
static Regular, Medium, SemiBold, and Bold weights. Their SIL Open Font License notices are
installed in the package's `licenses` directory.

## macOS

Supported: macOS 10.15.7 or later on Intel and Apple silicon.

1. Download the universal `.dmg`.
2. Open it and drag **Postman GPUI** to Applications.
3. Start Postman GPUI from Applications.

The zipped `.app` is also published for testers who do not want the DMG layout. Signed final builds
are notarized and should pass `spctl --assess --type execute` after installation. An unsigned
release candidate may require **Privacy & Security → Open Anyway**; the release notes will identify
it as unsigned.

## Windows

Supported: Windows 10 or later, x86_64.

1. Download the NSIS installer ending in `-setup.exe`.
2. Run the installer. The default current-user installation does not require administrator access.
3. Start Postman GPUI from the Start menu.

When the release is not Authenticode-signed, Windows SmartScreen can show an unknown-publisher
warning. Check the release notes and SHA-256 checksum before continuing.

## Linux

Supported release baseline: x86_64 distributions compatible with Ubuntu 22.04 libraries. A Vulkan
1.3-capable GPU/driver and either a Wayland or X11 desktop session are required.

### AppImage

```bash
chmod +x *Postman*GPUI*.AppImage
./*Postman*GPUI*.AppImage
```

### Debian/Ubuntu package

```bash
sudo apt install ./postman-gpui*.deb
```

The Debian package declares these runtime libraries: ALSA, Fontconfig, Vulkan loader, Wayland
client, XCB, and XKB Common/X11. On a minimal desktop, portals may also be needed for native file
dialogs:

```bash
sudo apt install libasound2 libfontconfig1 libvulkan1 libwayland-client0 \
  libxcb1 libxkbcommon0 libxkbcommon-x11-0 xdg-desktop-portal
```

To build on Debian/Ubuntu, install the development packages used by GPUI:

```bash
sudo apt update
sudo apt install build-essential clang cmake libasound2-dev libfontconfig1-dev \
  libwayland-dev libx11-xcb-dev libxkbcommon-x11-dev libvulkan1 pkg-config
```

## Verify a download

Each release publishes `SHA256SUMS`. From the directory containing the downloads:

```bash
sha256sum --check SHA256SUMS
```

On macOS, use `shasum -a 256 <artifact>` and compare it with the matching entry.

## Local application data

The app stores `postman-gpui/request-history.sqlite3` below the OS local-data directory:

- macOS: `~/Library/Application Support/postman-gpui/`
- Windows: `%LOCALAPPDATA%\postman-gpui\`
- Linux: `${XDG_DATA_HOME:-~/.local/share}/postman-gpui/`

Uninstalling the application does not automatically remove request history.
