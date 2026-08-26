# Cross-platform release smoke test

Run this checklist against the exact artifacts attached to the release candidate. Use a clean OS
account or virtual machine, not a development checkout. Record the OS version, artifact filename,
SHA-256, signing status, and pass/fail result in issue #148.

## Package integrity

- [ ] The downloaded artifact matches `SHA256SUMS`.
- [ ] The package installs without missing-file or missing-library errors.
- [ ] The installed application has the Postman GPUI icon and expected version.
- [ ] Heading, interface, and monospace text render correctly without installing extra fonts.
- [ ] Signed artifacts report a valid publisher/signature; unsigned candidates show the documented
      warning and no unexpected one.
- [ ] The application launches from the normal OS entry point, not from a terminal.

## Request workflow

- [ ] Send `GET https://httpbingo.org/get?source=release-smoke` and inspect status, headers, and body.
- [ ] Type the URL, a parameter, a header, authorization, and each body kind, then click Send while
      the last editor is still focused; the latest value is sent without Enter, Tab, blur, or Add.
- [ ] Send requests using multiple query/header rows and confirm disabled rows are omitted.
- [ ] Exercise Basic and Bearer authorization without persisting credentials to History.
- [ ] Send representative JSON, raw, URL-encoded, and multipart/file-upload bodies.
- [ ] Verify compression decoding and a multi-hop redirect with the selected redirect policy.
- [ ] Verify timeout and cancellation return the UI to a usable state.
- [ ] Verify non-2xx and transport failures render as errors rather than crashing the app.

## Native interaction and persistence

- [ ] Open two request tabs, edit both, switch between them, and confirm isolation.
- [ ] Copy response text and paste into a request editor using native shortcuts.
- [ ] Open the multipart file picker and select a local test file.
- [ ] Search History, replay a completed request, and send it again.
- [ ] Quit and relaunch; completed History remains available and sensitive values remain redacted.
- [ ] Uninstall succeeds. Existing local History behavior matches `docs/installation.md`.

## Platform-specific checks

### macOS

- [ ] The `.dmg` opens with a valid app bundle and the app can be copied to Applications.
- [ ] `codesign --verify --deep --strict` succeeds when the release is signed.
- [ ] `spctl --assess --type execute` succeeds when the release is notarized.

### Windows

- [ ] The current-user NSIS install and uninstall do not require administrator access.
- [ ] The Start-menu shortcut and installed executable display the correct icon.
- [ ] `Get-AuthenticodeSignature` reports `Valid` when signing secrets were configured.

### Linux

- [ ] The AppImage starts from an executable file on a supported Wayland session.
- [ ] The AppImage starts from an executable file on a supported X11 session.
- [ ] The Debian package installs dependencies and creates a desktop entry with the correct icon.
