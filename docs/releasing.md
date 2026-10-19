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
- Pin all packaging jobs to the source SHA resolved during validation. `BUILD-INFO-*.txt` binds
  each platform's tag, SHA, target, workflow attempt and actual signing status to `SHA256SUMS`.
- Published release assets are immutable. A new source revision or rebuilt published artifact
  needs a new candidate tag. Only an unpublished draft can resume uploads.
- Document current limitations in the changelog and release notes; issue closure alone does not
  establish that a feature is available in a shipped package.

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

## Build artifacts for acceptance before publication

Run the `Release` workflow with a candidate label in **tag**, the source branch/full commit in
**ref**, and **publish = false** (the default for manual runs). The candidate label must match
the base Cargo version; choose an unused label such as `v0.1.0-rc.N`. The checkout must contain
this workflow and its tooling. This mode does not create a tag or a GitHub Release.

Validation resolves the ref once, then all platform jobs check out that exact commit. The
`Verify candidate artifacts` job rejects missing/duplicate packages, mixed tag/commit/target/run
metadata and unexpected files. It generates `SHA256SUMS` and uploads one complete
`release-candidate-<tag>-<commit>-<attempt>` Actions artifact retained for 30 days. Keep the workflow run
and attempt URL with downloaded bytes; an Actions artifact is not a published release. Use
**Re-run all jobs** for a failed candidate: assembly rejects packages mixed across attempts, and
attempt-specific artifact names prevent a rerun from replacing earlier downloads.

Download and extract the complete artifact into a fresh directory, then verify it using the
same source revision's tooling:

```bash
python3 scripts/release_artifacts.py verify --directory candidate \
  --tag v0.1.0-rc.N --commit FULL_40_CHARACTER_COMMIT
```

The command requires all GUI/CLI packages, three build-info files and `SHA256SUMS`. It neither
creates missing checksums nor repairs changed bytes. For an individual package, compare its
SHA-256 with the manifest and retain its platform build-info file. Complete the platform matrix
in [the smoke checklist](release-smoke-test.md#candidate-evidence-249); #249 / #148 / #49 share
those results. Installed package, real file picker and OS IME evidence remain separate from CI.

## Prepare a release candidate

1. Confirm CI and HTTPBingo E2E are green on `main`.
2. Confirm the [editor-state regression checks](editor-state.md#regression-checks) pass on
   the commit being released. Use the installed-app smoke checklist for package-level verification.
3. Update `CHANGELOG.md` and review Known limitations against the code being released.
4. Choose an unused candidate tag matching the base version in `Cargo.toml`. In the commands below,
   replace `N` with the new candidate number, then validate release metadata:

   ```bash
   release_tag='v0.1.0-rc.N'
   python3 -m unittest discover -s scripts/tests
   python3 scripts/release.py verify --tag "$release_tag"
   ```

5. Create and push the annotated tag:

   ```bash
   git tag -a "$release_tag" -m "Postman GPUI $release_tag"
   git push origin "$release_tag"
   ```

6. The tag-triggered `Release` workflow builds a universal macOS `.app/.dmg`, Windows NSIS installer, Linux
   `.AppImage/.deb`, and standalone `postman-g` CLI archives for all three platforms. It publishes
   these with `BUILD-INFO-*.txt` and `SHA256SUMS`.
7. Complete `docs/release-smoke-test.md` on clean installations and record results in #148.

For a tag whose publication failed, use **Run workflow**, enter that tag, leave **ref** empty and
set **publish = true**. Publication validates that the checked-out SHA is exactly the tag's commit,
rechecks downloaded checksums, and permits only a new release or an unpublished draft. An already
published release is rejected before any asset upload or edit. Workflow runs for the same tag are
serialized. Source fixes need a new tag; rerunning an old tag does not pick up changes from `main`.

Tag publication rebuilds packages. Record the final published checksums and rerun affected installed
checks against those bytes; an artifact-only run with the same source SHA is not proof that a rebuilt
or newly signed package is byte-identical. Do not overwrite a published candidate to carry a fix.

## Promote v0.1.0

1. Resolve every release-candidate regression or document an accepted limitation.
2. Change the `0.1.0` changelog date from `Unreleased` to the release date.
3. Configure every macOS signing and notarization secret listed above. The release workflow rejects
   an unsigned final tag. Windows signing is strongly recommended; if it is unavailable, document
   the SmartScreen limitation in the final release notes.
4. Set `release_tag=v0.1.0`, then validate, create, and push the tag using the same commands.
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
cargo-packager. The runtime-assets check does not open a window; complete the X11 and Wayland
smoke checks separately. Publish a new release candidate to deliver the fix to users;
changing this script does not repair AppImages already attached to an older release.
