# Cross-platform release smoke test

Run this checklist against the exact artifacts attached to the release candidate. Use a clean OS
account or virtual machine, not a development checkout. Record the OS version, artifact filename,
SHA-256, signing status, and pass/fail result in issue #148.

## Candidate evidence (#249)

Record the candidate tag and full source commit, workflow run/attempt URL, package filename,
SHA-256, OS/version, architecture, display server, actual scale, signing/notarization status,
exact actions and observed results. Use the same record in #249, #148 and #49. Record manual
native checks separately from simulated controls, asset checks and CI build results.

| Installed environment | Package | Result / evidence |
| --- | --- | --- |
| macOS Apple silicon | Universal DMG/app | Pending |
| macOS Intel | Same universal DMG/app | Pending |
| Windows x86_64 | NSIS installer | Pending |
| Linux x86_64 Wayland | AppImage and Debian | Pending |
| Linux x86_64 X11 | AppImage and Debian | Pending |

Unavailable hardware, display scale, OS picker or IME checks stay Pending/Blocked with an owner
and follow-up. A test on the existing development account is not clean-account acceptance.
After a package fix, record the new bytes and repeat affected checks; never reuse an old pass
for an artifact with a different checksum. Use temporary captures and textual issue/PR evidence.

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
- [ ] Exercise Basic/Bearer auth and manually entered Authorization/AppToken headers; confirm History automatically retains the sent request without Save.
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
- [ ] Quit and relaunch; History restores sent headers, query values, auth, body and options for replay. Response previews remain sanitized, and the runtime Cookie Jar is not restored.
- [ ] Uninstall succeeds. Existing local History behavior matches `docs/installation.md`.

## Native Flow drafts (P5.1)

- [ ] Create two empty flows; Cancel/Escape a third and confirm no extra document appears.
- [ ] Append multiple HTTP steps and edit name, method, and URL while the field remains focused.
      Switch steps, flows, and Home/HTTP/Flows; unfinished text and selection remain isolated.
- [ ] Rename/filter flows and reopen a real entry from Home. Test Chinese IME and keyboard navigation.
- [ ] Drag library, inspector, and Console dividers; test arrows, Escape, numeric sizing, and reset.
      Shrink/expand the window and verify preferred sizes return without clipping controls.
- [ ] Exercise light/dark themes and narrow Canvas/Details mode. Verify Check/YAML, real file operations and Run/Cancel match the current capability descriptions.
- [ ] Restart and verify saved Flows reopen through Recent flows; unsaved drafts and run state are not restored;
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

## Flow Inputs (#224)

- [ ] Open Inputs in an empty flow and at compact width. Add definitions, toggle optional defaults,
  edit Text/JSON including absent vs null defaults, and check duplicate/blank-name errors.
- [ ] Set run overrides to empty Text, JSON null/false/0/object, then invalid JSON. Reset to inheritance.
  Verify invalid visible drafts never produce a usable prepared snapshot. No request is sent.
- [ ] Select/edit/cancel environments: only inherited declared base_url follows committed changes.
  Overrides survive, defaults and literal URLs stay unchanged, and runtime edits do not dirty YAML.
- [ ] Rename/delete/recreate declarations; confirm override ownership, no stale reused values, cached
  step hints, and preserved step expressions/drafts/undo. Switch flows and modes with Chinese IME.
- [ ] Mark synthetic values sensitive, reveal/edit, leave the context and return. Values are concealed;
  errors, Console, debug output and screenshots must not include the synthetic secret.
- [ ] Compare HTML/native Run inputs and Definitions in light/dark at the existing viewport/scale
  matrix, including many rows, long values, errors, reachable Add/Remove, and preserved Console.

## Flow Checks, Exports, and conditions (#222)

- [ ] In Step configuration, switch Request/Checks/Exports using pointer and focused-tab arrows,
      Home/End. Preserve drafts, independent scroll, and visible tab focus.
- [ ] Add two status checks; edit 100/599, clear one, enter 99/600/fraction/text, then undo/correct.
      Check counts follow valid definitions; readonly native check kinds retain order/content.
- [ ] Add/remove export rows; try blank/duplicate names and invalid JSONPaths, then correct them.
      Toggle Sensitive. Removing an earlier row preserves later input/undo and moves focus safely.
- [ ] Instantiate a later inspector before editing earlier exports; valid rename/removal updates
      its variable hints without resetting text/undo or rewriting references. Incomplete rows persist.
- [ ] Expand Run condition, paste native nested JSON, malformed shapes, then blank. Valid fields
      apply immediately, errors keep their last valid value. Verify clipboard, Chinese IME, multiline
      Enter, Tab/Shift+Tab and undo; no HTTP send or Console submission occurs.
- [ ] Keep different drafts in two steps/two flows, switch pages/themes/Console modes and resize.
      API/loop configuration remains readonly. Check/Run use the native compiler/engine and report real outcomes.
- [ ] Compare light/dark inspector states to the HTML at 1440×960; check 1920×1080, 1024×768,
      768×720, 1024×600 at 1×/2×. Long error/export content scrolls independently; actions remain
      reachable and the Console retains its bounds. Multiple checks and Sensitive are native additions.

## Flow Console (#215)

- [ ] At 1440 × 960, grow the inspector beyond 480px. Verify canvas space and restore sizes after
      shrinking to 1024 × 600 and expanding again. Test light/dark and compact Canvas/Details.
- [ ] Collapse/maximize/restore Console; selecting a tab reopens it. The command bar remains
      reachable, and its content scrolls within its own region without covering adjacent panels.
- [ ] Enter `help`, `logs`, `steps`, `outputs`, and `clear`; use both Enter and Cmd/Ctrl+Enter.
      Check command history, draft restoration, unique-prefix Tab completion, Shift+Tab, and Escape.
- [ ] Switch between two flows and Home/HTTP; logs and unfinished commands remain isolated.
      Clearing logs keeps history and document edits; copying includes only visible filtered logs.
- [ ] Verify Chinese IME does not submit while composing. `check` compiles without network I/O;
      `run` starts the native execution and `stop` cancels it. Unknown commands and unsupported
      arguments must never send requests or report successful execution.
- [ ] Scroll back through enough log entries to pause auto-scroll; re-enable it to follow the end.
      Steps/Outputs show not-run states and a fresh session has no fabricated logs or runs.

## Native complete Flow journey (#247)

- [ ] With a controlled local endpoint, open both `tests/fixtures/flows/echo-chain.http.yml` and
      `poll-status.http.yml`. Set the environment or a session `base_url` override. Check sends no
      requests; Run sends the expected GET → POST → PATCH JSON chain, and polling stops on `ready`.
- [ ] Select `crates/postman-flow/examples/httpbingo_multi_hop_chain.rs`: Open explains that Flow
      YAML is required. Recover using `examples/flows/httpbingo_multi_hop.http.yml`. Also try malformed
      YAML, long diagnostics, `.yaml` and extensionless saved documents. Existing drafts remain intact.
- [ ] Edit while focused, Check and copy YAML, Save as and Export, inspect Steps/Outputs, then
      close/quit/relaunch and reopen through Recent flows and Open. Compare full DSL semantics for
      the multi-hop, catalog/API and nested-loop fixtures. Session overrides/results must be absent.
- [ ] Match Home/HTTP/Flows and used overlays against the HTML in both themes at 1440×960,
      1920×1080, 1024×768, 768×720, 1024×600 and the native minimum 768×600. Record display scale.
      In compact windows, the environment settings icon remains labeled/keyboard accessible and
      file/execution controls fit; long file errors scroll without displacing the Console command bar.
- [ ] Record actual platform and process-restart evidence separately from test-window simulations.
      Record unavailable OS picker, display-scale and IME checks as follow-ups in #148 / #49;
      a development build does not satisfy installed-package acceptance.

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

## Flow step management (#226)

- [ ] In Request/Checks/Exports, move a middle step up/down. Check node numbers, stable selection,
  IDs, updated variable hints, and unchanged field text/undo. Canvas arrows still navigate.
- [ ] Check first/last/single-step boundaries. Remove shows name/ID; Cancel and Escape preserve
  drafts and focus. Confirm selects next/previous; removing the final step focuses Add step.
- [ ] Recreate after deletion: the new step has a new ID and no old editor state. Try two flows
  with identical step IDs; one flow's operations must not change the other or its Inputs overrides.
- [ ] Keep an invalid body/condition draft while reordering another step. Verify retained errors,
  clipboard, undo/redo and Chinese IME; Console typing must not reorder/delete steps.
- [ ] Check light/dark and compact/short windows at 1×/2×. Actions remain reachable, inspector
  fields scroll independently, and resizing/collapsing the Console preserves the editor.
- [ ] API/loop nodes move as complete units. Removing a producer does not rewrite downstream
  expressions or outputs; no compile/run success is claimed. Session drafts remain unsaved.

## HTTP Add to flow (#228)

- [ ] Type into focused HTTP URL/Params/Headers/Body/Auth/Options, including Chinese IME; add to a
  new flow without Send/Save. Verify method, repeated/empty query and headers, body and options.
- [ ] Add to an existing flow with another flow of the same name. Verify exactly one new step,
  preserved earlier editors/Inputs/Console, and Request focus; try Tab/Shift+Tab/Enter/Space/Escape.
- [ ] From Add step, select an inactive HTTP tab. Verify HTTP selection and drafts stay unchanged.
  Confirm New HTTP request still adds a blank step. Cancel/invalid name must not leave an empty flow.
- [ ] Reject changed/closed sources and multipart/Scripts/Tests with inline feedback. Verify
  Basic/Bearer, literal `{{text}}`, no-timeout and absolute file references without file reads or Send.
- [ ] Copy during an HTTP Send; its snapshot, eventual response and history must remain associated
  with the original tab. Edit each copy separately and check source undo and dirty/saved state.
- [ ] Check light/dark at 1440×960, 1920×1080, 1024×768, 768×720 and 1024×600, at 1×/2×.
  Search long lists/names, inspect long errors, and confirm the footer remains reachable and a compact
  successful transfer shows Details. Check native candidate windows with the platform input method.

### Flow Check and readonly YAML (#235)

Use local fixtures; Check must send no network request and read no Binary body file. For both
light/dark themes and 1×/2× display scales, inspect 1440×960, 1920×1080, 1024×768, 768×720,
and 1024×600. Compare Canvas/Console and YAML against `prototypes/request-workspace.html#flows`.

- Check an empty flow, then a valid local-URL flow from the toolbar and Console `check`. Verify
  failed/passed summaries, code/field/step/API diagnostics, and independent Not run results.
- Leave invalid URL/header/body/condition/status/export/input-definition drafts, switch step or
  Flow, then Check. Hidden drafts must block success. Invalid runtime overrides and missing run
  inputs must not. Fix back to the last applied value and confirm the prior result becomes stale.
- Activate Locate by mouse and keyboard from YAML, compact Details, collapsed and maximized
  Console. Check Request/Checks/Exports/Inputs focus and scrolling, readonly API/loop owners,
  root locations, and long/many diagnostics. Reorder/remove a step or row: old links must refuse.
- Copy YAML and parse it with the native codec; include API/loops/options/auth/conditions/native
  checks/inputs/defaults/outputs/layout. Confirm no runtime overrides/environment values enter it.
  Explicit sensitive document defaults remain. Compiler-invalid documents can still preview.
- Try typing/pasting/deleting in YAML; document stays unchanged. Switch Canvas/Flow/theme/size
  and back: unchanged source retains selection/scroll. Preview/Copy never saves a draft.
- With pending edits, retain the last applied source, show stale status, disable Copy. With no
  prior preview show a draft-error state. Exercise codec size/depth/layout errors, then recovery.

Run `cargo fmt -- --check`, workspace clippy with warnings denied, workspace tests, and the
cross-platform CI release-build jobs. External HTTPBingo tests remain a separate service smoke.

## Flow files and close protection (#237)

- [ ] On each tested package/OS, create/edit a Flow and Save through its focused input shortcut and
  toolbar. Check native picker cancellation, Unicode paths, readonly/missing directories, and visible errors.
- [ ] Restart on Home, reopen through Recent flows/Open, and verify exact native DSL semantics,
  including readonly API/loop/check fields, explicit defaults, outputs, auth/options and layout.
- [ ] Save as another filename: keep the first file and change the association. Export a copy after
  editing: keep the association and unsaved indicator. Reopen both copies and compare expected content.
- [ ] Leave invalid body/condition/Inputs edits in hidden panes and attempt all write actions. They
  must not silently save old values. Runtime input/environment changes must not enter saved YAML.
- [ ] Modify an associated file externally. Cancel conflict handling, then explicitly Replace file;
  check changed-again conflicts, failures and active-file destination collisions preserve both drafts.
- [ ] Close dirty Flows and mixed HTTP/Flow windows via keyboard, window close and normal Quit.
  Exercise Save, Discard changes, Cancel and cancelled pickers; late edits cannot be silently lost.
- [ ] Check saved/unsaved/saving state, long filenames, keyboard menus, dialog focus/Escape and
  light/dark layouts at reference, narrow and short sizes. Verify native IME before immediate Save.
- [ ] Remove a recent entry without deleting the file; test a missing recent file and damaged recent
  index recovery. Record the exact package/commit and distinguish installed evidence from headless tests.

## Native Flow execution (#239)

- [ ] Open a saved Flow against a controlled local service; set typed/environment inputs and Run.
      Verify real network traffic, step status, response timing, checks and terminal feedback.
- [ ] Cancel during HTTP and during a polling wait; later steps must not send. Rerun uses new inputs.
- [ ] Reject missing inputs and hidden invalid document/runtime JSON drafts before network activity.
- [ ] Switch Flows/modes, resize and change themes while running. Keep progress, edit state and focus.
      Verify old-revision result labels and navigation after document edits.
- [ ] Close a clean running Flow and a dirty running Flow; Cancel preserves execution. Save failures
      retain work; Stop and close waits for cleanup. Repeat with mixed HTTP/Flow window close and Quit.
- [ ] Copy filtered logs and inspect error/timeout paths for sensitive values. Clear keeps run results.
- [ ] Verify canvas/library Cmd/Ctrl+Enter, Console run/stop, text fields/IME and modal key ownership.
      Inspect typed Outputs with the checklist below.

## Native Flow result inspection (#241)

- [ ] Open a saved local-service Flow, Run, enter Steps and inspect a failed check, HTTP status/timing,
      extraction failure, skipped request and unexecuted child. Check diagnostics remain independent.
- [ ] Drill into nested for_each/repeat_until invocations. Verify one-based indexes, zero iterations,
      continue-after-failure, terminal reasons, polling interval/wait and cancellation during request/wait.
- [ ] Successfully rerun and inspect/copy `null`, `false`, `0`, empty strings/containers and nested JSON.
      Verify Copy value scope and Copy all outputs; failed/cancelled runs never expose final returns.
- [ ] Use synthetic secrets in annotations, nested JSON, auth/cookies, URL and response echoes. Check
      display/accessibility, JSON selection/copy and toolbar copy. Literal `[REDACTED]` is not marked redacted.
- [ ] Exercise large values/iterations: scroll retained lists, see omission counts/size limits and cancel.
      Switch Flows, views and themes without losing selection; Clear logs keeps results; rerun resets them.
- [ ] Compare native and fixture-only HTML light/dark 1440×960; resize to 1920×1080, 1024×768, 768×720,
      1024×600. Scroll/maximize Console; primary actions and command bar stay reachable. Test Up/Down,
      Enter, Tab, Escape, readonly selection/copy and prior-revision Locate protection.
- [ ] Repeat close/Quit/save failures; inspect YAML and HTTP history for absence of runtime results.
      Record screenshots/comparison notes as PR text only. P7 installed-platform/IME acceptance is separate.
