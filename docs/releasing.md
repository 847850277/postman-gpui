# Release runbook

This runbook covers `v0.1.0-rc.N` prereleases and the final `v0.1.0` release tracked by
[#148](https://github.com/847850277/postman-gpui/issues/148).

## Release invariants

- `Cargo.toml` is the source of truth for the base version.
- A tag must be `vMAJOR.MINOR.PATCH` or `vMAJOR.MINOR.PATCH-PRERELEASE` and its base version must
  exactly match `Cargo.toml`.
- `cargo-packager` is pinned to `0.11.8` in CI and local commands.
- Linux AppImages are built through `scripts/release.py package`. It verifies the upstream
  AppRun download, fixes its cached permissions to `0755`, and packages with umask `0022`.
  This works around cargo-packager 0.11.8's owner-only executable launcher permissions.
- After packaging, `unsquashfs` extracts the embedded filesystem with its original permissions;
  the launcher, main binary, and parent directories are checked for access by all users.
  Do not use `--appimage-extract` to inspect directory permissions: type2-runtime creates
  directories with mode `0700`. Release CI uses the verified extraction and executes its
  root-owned AppRun as a separate unprivileged user before upload.
- All macOS, Windows, and Linux jobs must finish before a GitHub Release is created. A failed job
  cannot publish a partial release.
- `v0.1.0-rc.N` is a GitHub prerelease. A tag without a suffix is a final release.
- #69 and its child issues remain pending and do not block v0.1.0.

## Repository secrets

The workflow produces unsigned prerelease packages when signing credentials are absent. Configure
these Actions secrets before publishing signed packages.

### macOS signing and notarization

| Secret | Value |
| --- | --- |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: NAME (TEAM_ID)` |
| `APPLE_CERTIFICATE` | Base64-encoded Developer ID `.p12` certificate |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` |
| `APPLE_ID` | Apple developer account email |
| `APPLE_PASSWORD` | App-specific password |
| `APPLE_TEAM_ID` | Apple Developer team ID |

`cargo-packager` imports the certificate into a temporary keychain, signs the `.app` and `.dmg`,
submits the application to Apple's notary service, and removes the temporary keychain.

### Windows Authenticode signing

| Secret | Value |
| --- | --- |
| `WINDOWS_CERTIFICATE` | Base64-encoded code-signing `.pfx` certificate |
| `WINDOWS_CERTIFICATE_PASSWORD` | Password for the `.pfx` |
| `WINDOWS_CERTIFICATE_THUMBPRINT` | SHA-1 certificate thumbprint used by SignTool |

The workflow imports the certificate into the current user's certificate store immediately before
packaging. Without these secrets, the NSIS installer is still produced but release notes must warn
about SmartScreen.

## Prepare a release candidate

1. Confirm CI and HTTPBingo E2E are green on `main`.
2. Review `docs/autofill-contract.md`; once the listed suite passes on the merged release branch,
   close #49 as complete.
3. Update `CHANGELOG.md` and confirm #69 remains listed under Known limitations.
4. Validate release metadata:

   ```bash
   python3 -m unittest discover -s scripts/tests
   python3 scripts/release.py verify --tag v0.1.0-rc.1
   ```

5. Create and push the annotated tag:

   ```bash
   git tag -a v0.1.0-rc.1 -m "Postman GPUI v0.1.0-rc.1"
   git push origin v0.1.0-rc.1
   ```

6. The `Release` workflow builds a universal macOS `.app/.dmg`, Windows NSIS installer, Linux
   `.AppImage/.deb`, then publishes them with `SHA256SUMS`.
7. Complete `docs/release-smoke-test.md` on clean installations and record results in #148.

## Promote v0.1.0

1. Resolve every release-candidate regression or document an accepted limitation.
2. Change the `0.1.0` changelog date from `Unreleased` to the release date.
3. Configure every macOS signing and notarization secret listed above. The release workflow rejects
   an unsigned final tag. Windows signing is strongly recommended; if it is unavailable, document
   the SmartScreen limitation in the final release notes.
4. Validate and push `v0.1.0` using the same commands.
5. Verify all release assets and checksums before announcing the release.

## Local package commands

```bash
cargo install cargo-packager --version 0.11.8 --locked

# Current native platform
python3 scripts/release.py package

# Universal macOS
python3 scripts/release.py package --universal-macos

# Print resolved metadata without building
python3 scripts/release.py config --platform linux --target x86_64-unknown-linux-gnu

# Verify that the native text backend loads all embedded fonts (no window or History database)
cargo run --locked --release --example verify_runtime_assets

# Recheck the entry point permissions of a built/downloaded AppImage (Linux, no FUSE needed)
sudo apt-get install --yes squashfs-tools
python3 scripts/release.py verify-appimage dist/postman-gpui_0.1.0_x86_64.AppImage
```

Use `verify-appimage <file.AppImage> --extract-to <new-directory>` to retain the verified
filesystem for a smoke test. The release-tooling tests build actual SquashFS fixtures, including
bad permissions; install `squashfs-tools` on Linux or `brew install squashfs` on macOS to run
these tests. CI installs the tools explicitly.

The permission workaround is applied on both fresh and existing packager caches. A launcher
checksum mismatch stops packaging; investigate the cached file or upstream release rather than
silently trusting new bytes. Revisit the workaround and pinned launcher checksums when upgrading
cargo-packager. The automated runtime-assets check does not open a window; complete the X11 and
Wayland smoke checks separately. Publish a new release candidate to deliver the fix to users;
changing this script does not repair AppImages already attached to an older release.
