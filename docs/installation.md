# Installation

Postman GPUI supports macOS, Windows, and Linux. Download artifacts from
[GitHub Releases](https://github.com/847850277/postman-gpui/releases).

Both final releases and release candidates may be unsigned. Their release notes state that
explicitly; do not bypass an operating-system security warning unless the artifact and its
SHA-256 checksum came from this repository's release page.

Inter and JetBrains Mono are embedded for consistent cross-platform text rendering. Inter includes
static Regular, Medium, SemiBold, and Bold weights. Their SIL Open Font License notices are
installed in the package's `licenses` directory.

## macOS

Supported: macOS 10.15.7 or later on Intel and Apple silicon.

1. Download the universal `.dmg`.
2. Open it and drag **Postman GPUI** to Applications.
3. Start Postman GPUI from Applications.

The zipped `.app` is also published for users who do not want the DMG layout. Signing and
notarization are optional and recorded separately in the release notes and build information.
A notarized build should pass `spctl --assess --type execute` after installation. An unsigned or
unnotarized build can trigger Gatekeeper, including when its version has no prerelease suffix.
Verify the download's origin and checksum before using the OS's application-opening controls.

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
chmod +x ./postman-gpui_*.AppImage
./postman-gpui_*.AppImage
```

### Debian/Ubuntu package

```bash
sudo apt install ./postman-gpui*.deb
```

The Debian package declares these runtime libraries: ALSA, Fontconfig, Vulkan loader, Wayland
client, XCB, and XKB Common/X11. Native Open, Save as and Export dialogs require a running
user D-Bus session, `xdg-desktop-portal`, and a file chooser backend for your desktop
(for example `xdg-desktop-portal-gtk` on GTK desktops or `xdg-desktop-portal-kde` on KDE):

```bash
sudo apt install libasound2 libfontconfig1 libvulkan1 libwayland-client0 \
  libxcb1 libxkbcommon0 libxkbcommon-x11-0 xdg-desktop-portal
```

A `ZBus` connection error mentioning `/run/user/<uid>/bus` means the desktop session bus is
missing or its address is stale. Launch the app from a logged-in graphical desktop session;
check that `DBUS_SESSION_BUS_ADDRESS` belongs to that session. Install/enable the matching
portal backend if the bus works but the file chooser is unavailable. A bare SSH/container/Xvfb
session does not supply these services just by displaying a window; `dbus-run-session` alone
also does not install a portal backend. Failed or cancelled pickers retain the in-memory Flow.
For headless execution, use `postman-g` with a Flow YAML file instead of the desktop dialogs.

To build on Debian/Ubuntu, install the development packages used by GPUI:

```bash
sudo apt update
sudo apt install build-essential clang cmake libasound2-dev libfontconfig1-dev \
  libwayland-dev libx11-xcb-dev libxkbcommon-x11-dev libvulkan1 pkg-config
```

## Postman CLI (`postman-g`)

`postman-g` is the headless CLI runner for `.http` requests and `.http.yml` workflows.

### One-line installer (macOS & Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/847850277/postman-gpui/main/install.sh | bash
```

### Install with Cargo

```bash
# Install directly from GitHub
cargo install --git https://github.com/847850277/postman-gpui postman-cli --bin postman-g

# Or build from local checkout
cargo install --path crates/postman-cli --bin postman-g
```

### Download prebuilt binary

Download standalone prebuilt binaries from [GitHub Releases](https://github.com/847850277/postman-gpui/releases):

- macOS (universal): `postman-g-macos-universal.tar.gz`
- Linux (x86_64): `postman-g-linux-x86_64.tar.gz`
- Windows (x86_64): `postman-g-windows-x86_64.zip`

Extract and place `postman-g` in your system `PATH` (such as `~/.cargo/bin` or `/usr/local/bin`).

## Verify a download

Use one candidate's complete artifact set. Each `BUILD-INFO-<platform>.txt` records its tag,
full source commit, target, signing/notarization status and workflow run/attempt. The metadata
and packages are covered by `SHA256SUMS`; keep their identity with your smoke-test results.
A successful runtime-assets check verifies embedded fonts, not interactive platform acceptance.
Older candidates (including rc.4) do not include the later native Flow work described by current
source documentation. See the selected release's notes before comparing capabilities.

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
