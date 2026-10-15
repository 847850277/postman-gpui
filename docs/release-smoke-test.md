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
- [ ] Switch between Home, HTTP, and Flows without losing HTTP drafts or in-flight requests.
- [ ] Save and rename a request, restart, and reopen it from Home and search with its body,
      authorization, scripts, options, and notes intact.
- [ ] Modify a saved request, then exercise Save, Discard changes, and Cancel when closing
      the tab or quitting; cancellation keeps the edited request open.
- [ ] Import the HTTPBingo cURL example, inspect its preview, send it, and copy its Code export.
- [ ] Change and persist an environment; matching URLs rebase without losing paths or queries.
- [ ] Resize the request/response split in both arrangements, reset it, and check the saved
      layout after restarting. Scrollbars remain attached to the visible editor viewport.
- [ ] Switch light/dark appearance and response Pretty/Raw modes; verify JSON highlighting,
      wrapping, selection, and copy. The response editor must remain read-only.
- [ ] Copy response text and paste into a request editor using native shortcuts.
- [ ] Open the multipart file picker and select a local test file.
- [ ] Search History, replay a completed request, and send it again.
- [ ] Delete one History entry, cancel Clear all, then confirm Clear all; saved requests and
      open drafts remain intact.
- [ ] Quit and relaunch; completed History remains available and sensitive values remain redacted.
- [ ] Uninstall succeeds. Existing local History behavior matches `docs/installation.md`.

## Native Flow drafts (P5.1)

- [ ] Create two empty flows; Cancel/Escape a third and confirm no extra document appears.
- [ ] Append multiple HTTP steps and edit name, method, and URL while the field remains focused.
      Switch steps, flows, and Home/HTTP/Flows; unfinished text and selection remain isolated.
- [ ] Rename/filter flows and reopen a real entry from Home. Test Chinese IME and keyboard navigation.
- [ ] Drag library, inspector, and Console dividers; test arrows, Escape, numeric sizing, and reset.
      Shrink/expand the window and verify preferred sizes return without clipping controls.
- [ ] Exercise light/dark themes and narrow Canvas/Details mode. Verify Check, YAML, Export,
      and Run remain unavailable; no simulated result or successful save is reported.
- [ ] Restart and verify Flow drafts are gone, consistent with the visible session-only notice;
      saved HTTP requests, environment settings, and history retain their existing behavior.

## Flow Request details (#218)

- [ ] Edit name, method, literal URL (including literal `{{...}}`), and URL expression without blur.
      Test `concat`, `coalesce`, `calc`, input/output references and invalid JSON. Complex expressions
      must not switch to Literal mode. Do not mistake shape feedback for successful compilation.
- [ ] Expand Headers; test duplicate names, empty values, embedded colons, Unicode, and missing
      separators. Switch literal/expression modes without losing semantics. Clear explicitly.
- [ ] Edit Body `none`, typed `json`, `json_template`, `raw`, `url_encoded`, and `file`; keep typed
      booleans/numbers/null/literal objects intact. File paths must not trigger filesystem access.
      Unknown kinds/fields and malformed JSON stay visible while the last valid body is retained.
- [ ] Leave different invalid drafts in two steps and two flows. Switch Home/HTTP/Flows, themes,
      and Console sizes; return, correct, undo/redo, select/copy/paste, enter newlines, and use Chinese
      IME and Tab/Shift+Tab. No editor keystroke submits Console commands or sends HTTP requests.
- [ ] Check available-variable names against declared inputs and preceding exports; no default,
      runtime, secret values, or later-step exports appear. API/loop request sources remain read-only.
- [ ] Compare HTML/native Request details at 1440 × 960 in light/dark; exercise 1920 × 1080,
      1024 × 768, 768 × 720, and 1024 × 600 at 1×/2×. Scroll long headers/body/errors independently
      of Console. Verify narrow Details mode, visible focus, mode hit areas, and clipboard/undo.

## Flow Console (#215)

- [ ] At 1440 × 960, grow the inspector beyond 480px. Verify canvas space and restore sizes after
      shrinking to 1024 × 600 and expanding again. Test light/dark and compact Canvas/Details.
- [ ] Collapse/maximize/restore Console; selecting a tab reopens it. The command bar remains
      reachable, and its content scrolls within its own region without covering adjacent panels.
- [ ] Enter `help`, `logs`, `steps`, `outputs`, and `clear`; use both Enter and Cmd/Ctrl+Enter.
      Check command history, draft restoration, unique-prefix Tab completion, Shift+Tab, and Escape.
- [ ] Switch between two flows and Home/HTTP; logs and unfinished commands remain isolated.
      Clearing logs keeps history and document edits; copying includes only visible filtered logs.
- [ ] Verify Chinese IME does not submit while composing. `check`, `run`, `stop`, unknown commands,
      and unsupported arguments must never send requests or report successful execution.
- [ ] Scroll back through enough log entries to pause auto-scroll; re-enable it to follow the end.
      Steps/Outputs show not-run states and a fresh session has no fabricated logs or runs.

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

- [ ] `python3 scripts/release.py verify-appimage <file.AppImage>` passes: packaged `AppRun`
      and `usr/bin/postman-gpui` both have mode `0755`.
- [ ] The root-owned extracted AppRun executes `--verify-runtime-assets` as a different,
      unprivileged user without changing any extracted executable permissions (automated in CI).
- [ ] The AppImage starts from an executable file on a supported Wayland session.
- [ ] The AppImage starts from an executable file on a supported X11 session.
- [ ] The Debian package installs dependencies and creates a desktop entry with the correct icon.
