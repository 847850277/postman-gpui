# Cross-platform release smoke test

Run this checklist against the exact artifacts attached to the release candidate. Use a clean OS
account or virtual machine, not a development checkout. Record the OS version, artifact filename,
SHA-256, signing status, and pass/fail result in issue #148.

## Current acceptance slice (#261)

Status snapshot: **November 11, 2026**. #261 continues the post-#260 installed acceptance
work; #249 is historical evidence, not proof that the five environments passed. #148 remains
the release tracker and #194 P7-2 remains the UI/platform tracker. This work currently covers
the available Apple silicon development machine and automation; other environments are unavailable.

### Source, CI, and publication identity

| Revision | Verified evidence |
| --- | --- |
| #260 baseline `955249fb6e07249de232b4137589c654c257ae1b` | [CI](https://github.com/847850277/postman-gpui/actions/runs/42784547654) and [Meilisearch E2E](https://github.com/847850277/postman-gpui/actions/runs/42784547627) completed successfully. |
| Committed base `118f6c447813e7f94184c9c91a5bd1b890c1a709` | All 10 [CI jobs](https://github.com/847850277/postman-gpui/actions/runs/42979012995) passed; [Meilisearch](https://github.com/847850277/postman-gpui/actions/runs/42979013031) and [Vaultwarden](https://github.com/847850277/postman-gpui/actions/runs/42979012858) also passed. |
| Additional changes after #260 | README tours at `7061d52` and `dfbf742`; pinned/checksummed CLI installer #262 / #263 at `118f6c4`. No later HTTP/Flow implementation change is included in this local package. |
| Latest published release | [v0.1.3](https://github.com/847850277/postman-gpui/releases/tag/v0.1.3), source `64f7a818a01c54faedfaa042d05fe49a9567c84c`; it does **not** contain #260. |
| Selected post-#260 release candidate | **Pending**: no tag/published package set has been selected. The local package below has no release tag or release workflow run. |

Do not reuse these CI results for a later source commit. The new acceptance tooling and CI
steps in this slice need their own revision's checks before inclusion in a published candidate.

### Local Apple silicon evidence (not clean-install acceptance)

Built with the existing release packager from `118f6c447813e7f94184c9c91a5bd1b890c1a709`.
This first package predates the cURL conversion fix described below:

```bash
python3.12 scripts/release.py package --platform macos \
  --target aarch64-apple-darwin --formats app --out-dir /tmp/postman-gpui-261/local-package
```

- Environment: macOS 14.6 (23G80), arm64, existing development account. The thin arm64
  app is **not** the universal DMG/app release artifact. Clean account, installed-package
  launch/uninstall, actual display-scale measurement and complete viewport coverage remain pending.
- Package: `Postman-GPUI_0.1.3_aarch64.app.zip`, SHA256
  `ae3f7518c8e3b28df264e4f31520f09f8164e12476e552c27536253a475b734b`.
- App executable SHA256: `36f5e971c927f861f542a233b1d6141dc876c52a671b3dafd00653c0563018a4`.
  `codesign -dv` reports linker ad-hoc signing, no TeamIdentifier and no sealed resources;
  this is not Developer ID signing or notarization. `--verify-runtime-assets` exited 0.
- Actual native Open selected `echo-chain.http.yml`. Check passed with zero fixture requests.
  With a session `base_url=http://127.0.0.1:8765`, Run sent GET `/uuid`, POST `/post`,
  PATCH `/patch`; captured JSON propagated the controlled UUID and returned `status=active`.
- Copy all outputs followed by native Cmd+V into the Console field retained the real output
  JSON. The pasted content was cleared without execution.
- Native Save as wrote a temporary Flow copy. A Unicode title was saved with Cmd+S;
  Cmd+Q and a new application process reopened it through Recent flows. The title and steps
  survived, both session override checkboxes reset, and saved YAML contained no loopback override.
- Native HTTP Send delivered identical `native body 中文\n` bytes (19 bytes) for GET, DELETE,
  HEAD and OPTIONS. OPTIONS with explicit-empty Raw emitted zero bytes and `Content-Type:
  text/plain`; switching to None emitted zero bytes without that generated header. These were
  controlled synthetic requests; no saved user request was executed.
- Actual Chinese IME composition is **Pending**: Unicode persistence was observed, but a
  native candidate/composition session was not established. Unicode insertion is not IME evidence.
- Source automation passed in separate runs: 351 GUI/library/history/timeout tests, 179 remaining
  integration tests, 194 engine/CLI/MCP tests, and 13 external HTTPBingo tests. The first
  HTTPBingo run failed because of the unavailable local proxy; rerunning with proxy variables
  removed passed all 13. Remote service E2E results are linked above, separately from local tests.
- The local release CLI archive `postman-g-macos-aarch64-local.tar.gz`, SHA256
  `664072f6a2722a7ba7c845f482101e513b384206ee85ed27ac6b065b95525eae`, passed the new
  checksum/extraction smoke with 35 body cases and both Flow fixtures. Its binary SHA256 is
  `561a535f4a5cc9bbb50e4ddb14654d7ca0cc30b66b4c90b2a0078f7d63d93f07`. This local archive
  also has no release tag/workflow identity and is not the universal published CLI.
- All 55 Python tooling tests (including 15 new smoke-tool tests), `cargo fmt --all -- --check`,
  workspace/all-target/all-feature Clippy with warnings denied, and `git diff --check` passed.

Temporary logs, server captures and the local app are under `/tmp/postman-gpui-261`; they are
not committed or durable release artifacts. The hashes and observed actions above identify this
local run. Repeat the affected checks on the eventual published candidate before marking a row passed.

### cURL regression discovered during local acceptance

The first direct run of cURL 8.16.0 covered seven methods, None/empty/Unicode bodies and
301/302/303/307/308. Seven of 105 combinations differed from native behavior: bodyless POST
retained POST on 301/302/303, and bodyless PUT/PATCH/DELETE/OPTIONS retained their methods on 303.
`--follow` alone does not fix these combinations because cURL's underlying transfer is already GET.
The system cURL 8.7.1 was not used for this check. The test executable was built temporarily from
[official cURL 8.16.0 source](https://curl.se/download/curl-8.16.0.tar.xz), SHA256
`40c8cddbcb6cc6251c03dea423a472a6cea4037be654ba5cf5dec6eb2d22ff1d`, with zlib and no TLS backend;
only controlled loopback HTTP was exercised.

The working-tree fix rejects importing/exporting those bodyless custom methods with Follow
redirects, with an actionable error. It preserves None rather than silently adding an empty body.
Explicit-empty bodies and disabling redirects remain supported; see [the corrected contract](curl.md).
The regression test failed on the old code and passed with the guard. The actual exporter now
passes 150 comparisons against native transport: seven methods, None/empty/Unicode/binary-file/
empty-file bodies (excluding the five rejected bodyless combinations), across all five redirect
statuses. Both clients must reach the expected method and byte matchers on both hops.

Reproduce with a cURL 8.16+ executable that supports compression:

```bash
POSTMAN_TEST_CURL=/absolute/path/to/curl cargo test --locked --lib \
  models::curl::tests::exported_commands_match_native_redirects_with_real_curl \
  -- --ignored --nocapture
```

The real-cURL test is explicit because arbitrary system cURL versions cannot exercise `--follow`.
CI builds the pinned, checksum-verified 8.16.0 source and runs it in the Build and Test job.
Default tests cover conversion rejection/recovery and the real Kit controls for disabled Import/
Copy, unchanged clipboard/drafts and recovery with an explicit-empty body. These automated UI
checks are separate from native installed-package acceptance.

After the fix, 345 library tests and 10 cURL UI tests passed, followed by the explicit real-cURL
test above. Formatting and workspace/all-target/all-feature Clippy also passed. These reruns
supplement the baseline test counts above; they are not an additional 355 distinct baseline tests.

The fixed working tree was repackaged with the same command into
`/tmp/postman-gpui-261/fixed-package`. Its `Postman-GPUI_0.1.3_aarch64.app.zip` SHA256 is
`28d4479761b4544f88a4c46a040e2621238f4284e2ffb2d006536fe6ab68e898`, and executable SHA256 is
`d9f825be58ade2209533107ea96be6676215f9ec6918e9159e173ae729b734e9`.
Runtime asset verification exited 0. Native desktop verification launched that exact app path:
DELETE with None showed the explanatory error and disabled Copy; importing bodyless
`--follow -X DELETE` showed the same error and disabled Import. Adding explicit-empty data and
`--post301 --post302` enabled import, and Code displayed the flags plus the cURL 8.16+ note.
Copy code followed by native Cmd+V into Import retained the generated command and produced
a valid preview. Both synthetic drafts were discarded through the normal quit prompts.
This is still a thin development-account package with no candidate tag or release workflow;
the earlier package's complete HTTP/Flow observations are not relabeled as observations of this fix.

### Tracker reconciliation and remaining ownership

#148's October rc.4/pre-v0.1.0 snapshot is historical: final releases through v0.1.3 have since
been published, and final releases may be unsigned under the current documented policy.
Signing/notarization must be reported from the candidate's BUILD-INFO and actual OS checks.
Current Flow Check/YAML, file persistence, Run/Cancel and detailed results are implemented;
they must not be described as future P5/P6 work. None of those facts completes installed acceptance.
#194 should point P7-2 at #261 while retaining #249 and #248 as historical evidence. Keep both
trackers and #261 open until the shared candidate-bound matrix is complete.

Release maintainer `@847850277` owns selection/publication of a post-#260 candidate and arranging
the unavailable machines. Follow-up order remains Linux Wayland/X11 (both package formats),
Windows NSIS, then both macOS architectures. Current local evidence can guide the runs but cannot
transfer a pass to those environments.

## Candidate evidence record

Record the candidate tag and full source commit, workflow run/attempt URL, package filename,
SHA-256, OS/version, architecture, display server, actual scale, signing/notarization status,
exact actions and observed results. Use the same record in #261, #148, #194 and #49. Preserve
the earlier #249 record. Record manual
native checks separately from simulated controls, asset checks and CI build results.

| Installed environment | Package | Result / evidence |
| --- | --- | --- |
| macOS Apple silicon | Universal DMG/app | Pending — local thin-app evidence above only; maintainer to repeat on a clean account with the selected candidate, native IME and measured scale. |
| macOS Intel | Same universal DMG/app | Pending — hardware unavailable; maintainer to arrange an Intel machine and run all native checks. |
| Windows x86_64 | NSIS installer | Pending — hardware unavailable; maintainer to arrange a Windows desktop and run install/native/uninstall checks. |
| Linux x86_64 Wayland | AppImage and Debian | Pending — desktop unavailable; maintainer to arrange a Wayland session and verify both packages. |
| Linux x86_64 X11 | AppImage and Debian | Pending — desktop unavailable; maintainer to arrange an X11 session and verify both packages. |

Unavailable hardware, display scale, OS picker or IME checks stay Pending/Blocked with an owner
and follow-up. A test on the existing development account is not clean-account acceptance.
After a package fix, record the new bytes and repeat affected checks; never reuse an old pass
for an artifact with a different checksum. Use temporary captures and textual issue/PR evidence.

For each environment, fill in this identity before starting, then append exact actions, expected
results, observed results and evidence references for each checkbox:

```text
tag / full source commit / release URL:
release workflow run + attempt / exact-revision CI URLs:
package filename / verified SHA256 / BUILD-INFO target:
OS version / architecture / display server / actual scale:
clean account or development account / installed path:
signing + notarization or Authenticode result:
tester / timestamp / curl --version:
automated checks (separate):
native checks (separate):
remaining Pending/Blocked checks / owner / next action:
```

## Repeatable controlled checks

Python 3.11+ is required (CI uses 3.12). Run the same server on each desktop using only
synthetic fixtures. It listens on loopback, records actual method/headers/body bytes as JSONL,
and refuses to reuse an existing capture path:

```bash
python3 scripts/release_smoke.py serve --port 8765 --log /tmp/postman-native-requests.jsonl
```

On Windows, use an existing temporary directory (PowerShell); choose a new log filename for
each session because the server refuses to append to a previous capture:

```powershell
python scripts/release_smoke.py serve --port 8765 --log "$env:TEMP\postman-native-requests.jsonl"
```

Set the two existing Flow fixtures' session `base_url` to `http://127.0.0.1:8765`. The server
supports `/uuid`, `/post`, `/patch`, `/echo`, `/anything/health?status=ready`,
`/redirect/301/echo` (also 302/303/307/308, nestable), `/status/500` and `/delay?seconds=2`.
Polling returns `pending` then `ready`; restart the server with a new log for a fresh polling run.
HEAD captures the request body but returns no response body and Content-Length 0, so explicit
HEAD cURL requests do not wait for nonexistent response bytes.

The CLI check uses the real executable, tests 35 method/body combinations (None, explicit-empty
Raw, Unicode Raw, binary file and empty file across all seven methods), and runs the existing
echo-chain and polling fixtures. Check must send no traffic; actual methods and bytes must match.
The CLI subprocess drops HTTP/HTTPS/ALL proxy environment variables in either case while retaining
unrelated settings. The proxy-isolation regression runs the real CLI with a recording proxy enabled
and requires all 40 fixture requests to arrive directly, with zero requests at the proxy.

```bash
# Source-build evidence only; record the binary SHA256 printed in the JSON report.
cargo build --locked -p postman-cli --bin postman-g
python3 scripts/release_smoke.py cli --binary target/debug/postman-g --output /tmp/cli-smoke.json

# Release-archive evidence: use files from ONE release, already downloaded together.
python3 scripts/release_smoke.py cli --archive postman-g-macos-universal.tar.gz \
  --checksums SHA256SUMS --build-info BUILD-INFO-macos.txt --output cli-smoke.json
```

On Windows use `postman-g.exe` or `postman-g-windows-x86_64.zip` and `BUILD-INFO-windows.txt`.
For example, in PowerShell:

```powershell
python scripts/release_smoke.py cli --binary target/debug/postman-g.exe --output "$env:TEMP\cli-smoke.json"
python scripts/release_smoke.py cli --archive postman-g-windows-x86_64.zip --checksums SHA256SUMS --build-info BUILD-INFO-windows.txt --output "$env:TEMP\cli-smoke-release.json"
```

For Linux use its x86_64 archive and `BUILD-INFO-linux.txt`. Archive and optional BUILD-INFO
hashes are checked before use. With BUILD-INFO, the archive's release filename must also match
the metadata filename and target, including `universal-apple-darwin` for the macOS universal
archive; mismatches fail before CLI execution. Custom archive names remain supported without
`--build-info`, and neither that mode nor `--binary` claims verified build identity.
The report retains package/binary hashes, source/tag/workflow
metadata when supplied, fixture identities and observed requests. A standalone CLI pass is not
evidence for the desktop editor, file picker, composition, signing or installation. Release CI
publishes `CLI-SMOKE-<platform>.json` alongside BUILD-INFO, covered by the final SHA256SUMS;
regular three-platform CI also runs the controlled CLI checks.
Malformed CLI reports (including non-object JSON) and temporary-directory cleanup errors replace
any prior success report with `success=false` and a diagnostic, and exit nonzero. The release
publication gate requires exactly one smoke report for each of the three platforms.

### #260 Body and cURL acceptance

- [ ] In the installed HTTP editor, Send DELETE/GET/HEAD/OPTIONS with focused JSON/Raw editors;
  compare received methods and bytes. Repeat None, explicit-empty Raw and file uploads including
  empty and nonempty HEAD files. Replay legacy bodyless History entries without adding a body.
- [ ] Import/export the combinations in [the pinned cURL contract](curl.md). Reject incompatible
  explicit POST/PUT/PATCH/DELETE/OPTIONS with `--location`, and show the cURL 8.16+ requirement
  for `--follow`. Record the executable's actual `curl --version` before testing exported commands.
- [ ] Observe both hops for 301/302/303 (plus 307/308): non-POST bodies retain bytes on 301/302;
  HEAD also retains them on 303. Confirm `--post301`, `--post302` and HEAD's `--post303` in exports,
  reject imports missing required flags, and check method transitions against the documented contract.
- [ ] Reject bodyless POST/PUT/PATCH/DELETE/OPTIONS with Follow redirects, including `--get`
  query-only data. Verify that an explicit empty body or disabling redirects resolves the error.
- [ ] Keep desktop import/export evidence separate from running cURL directly or CLI transport
  tests. This machine's system cURL is 8.7.1; the temporary 8.16.0 build and observed redirect
  comparisons are recorded above. Never substitute `--location` for a `--follow` check.

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
