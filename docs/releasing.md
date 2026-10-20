# Release runbook

This runbook covers versioned prereleases and final releases. Platform acceptance is tracked in
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
- A tag with a suffix such as `-rc.1` is a GitHub prerelease. A tag without a suffix is a final release.
- Pin all packaging jobs to the source SHA resolved during validation. `BUILD-INFO-*.txt` binds
  each platform's tag, SHA, target, workflow attempt and actual signing status to `SHA256SUMS`.
- Published release assets are immutable. A new source revision or rebuilt published artifact
  needs a new release tag. Only an unpublished draft can replace uploads; an empty, manually
  created release can receive its first artifact set without overwriting any assets.
- Document current limitations in the changelog and release notes; issue closure alone does not
  establish that a feature is available in a shipped package.

## Repository secrets

Signing and notarization are optional for both final releases and prereleases. With no signing
credentials, the workflow produces unsigned packages and marks that in build metadata and release
notes. Configured signing must still pass verification; incomplete credentials fail validation.
Configure these Actions secrets when signed packages are required.

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

## Publish a release

The workflow uses the existing `scripts/release.py` directly. Push a version tag, or run the
`Release` workflow manually with an **existing tag**. A tag such as `v0.1.0` publishes a final
GitHub Release; a suffix such as `v0.1.0-rc.5` marks it as a prerelease. A prior RC is not required
by the workflow. There is no separate candidate-artifact job, source-ref input or publish toggle.

1. Confirm CI and the release's agreed smoke checks pass on the selected source commit. Review
   [editor-state checks](editor-state.md#regression-checks), current limitations and the changelog.
2. Choose an unused version. Set `[workspace.package].version` in `Cargo.toml`, refresh the
   workspace packages in `Cargo.lock` with `cargo update --offline --workspace`, and add a dated
   `## [VERSION]` entry to `CHANGELOG.md`. Review the lockfile diff to confirm external dependencies
   are unchanged. Validate the matching tag before pushing it:

   ```bash
   release_tag='v0.1.1'
   python3 -m unittest discover -s scripts/tests
   python3 scripts/release.py verify --tag "$release_tag"
   ```

3. Commit all required files before tagging. A local untracked file is not included in CI.
   Create and push the annotated tag:

   ```bash
   git tag -a "$release_tag" -m "Postman GPUI $release_tag"
   git push origin "$release_tag"
   ```

4. Validation resolves the tag's exact source SHA. All platform jobs use that SHA and verify
   their native packages. Signing/notarization checks run when their credentials are configured.
5. After all platforms finish, the publish job checks the complete GUI/CLI artifact set,
   generates and verifies `SHA256SUMS`, uploads a draft, then publishes it. Each platform's
   `BUILD-INFO` records the source SHA, workflow attempt and signing/notarization status.
6. Verify the downloaded release assets and checksums. Record installed-app evidence in #148 / #249
   using [the smoke checklist](release-smoke-test.md#candidate-evidence-249). A successful packaging
   job does not by itself establish native input or installation acceptance on every platform.

## Retry a failed publication

For a transient failure on an unchanged tag, use **Re-run all jobs**, or manually run `Release`
with that tag. Workflow runs for a tag are serialized, and attempt-specific artifact names keep
packages from earlier attempts out of the release. Only an unpublished draft can replace uploads.
A published release with no assets can receive its first set without `--clobber`; its existing
changelog is preserved and signing disclosures are appended. A published release that already
has assets is rejected before any upload or edit.

Source fixes must be included in the tag being built. Pushing a fix to `main` and rerunning an
old tag still checks out the old source. For a workflow-only fix, manually run the corrected
workflow from `main` with the existing tag; application sources still come from that tag.
Application source fixes need a new version/tag; retargeting an unpublished tag requires an
explicit maintainer decision. Never overwrite a published release to carry a fix.
Record rebuilt package checksums and repeat affected installed checks against those exact bytes.

Unsigned final releases remain final releases. Their notes must clearly identify macOS
Gatekeeper / Windows SmartScreen limitations, and macOS signing must not imply notarization.

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
