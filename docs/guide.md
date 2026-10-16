# Postman GPUI guide

[Project overview](../README.md) · [中文指南](guide-zh.md)

Installation, desktop usage, Flow DSL, development, and current limitations.
All shell commands below assume the repository root unless noted otherwise.

## Project components

| Component | Current capabilities | Entry point |
| --- | --- | --- |
| Native desktop app | HTTP editing, response inspection, local history, and session Flow drafts | `cargo run --locked` |
| Flow DSL and CLI | Compile and execute `.http` requests and `.http.yml` workflows | [postman-g](../crates/postman-cli/README.md) |
| Flow MCP server | Schema, examples, validation, inspection, and YAML generation for agents | [postman-flow-mcp](../crates/postman-flow-mcp/README.md) |
| HTML UI prototype | Separate Home, HTTP, and Flows screens with simulated responses | [Preview guide](../prototypes/README.md) |

The native app opens Home with separate HTTP and Flows destinations. Flows supports a native
library, sequential canvas, and HTTP Request details with native expressions. Executable workflows
use the CLI or Rust API.

Runnable HTTP examples use [HTTPBingo](https://httpbingo.org/) throughout the app,
prototype, and CLI/Flow guides. The HTML prototype uses local fixtures for these
examples; native and CLI requests contact the configured URL when executed.

## Desktop HTTP features

- GET, POST, PUT, PATCH, DELETE, HEAD, and OPTIONS requests
- Query parameters, custom headers, Basic/Bearer authorization, and cookies
- JSON, raw, URL-encoded, and multipart request bodies with file upload
- Redirect policy, response decompression, timeout, and cancellation controls
- Response status, headers, formatted body, and quick copy
- Multi-tab requests, global search, and replayable SQLite history
- Shared, persistent HTTP/Flows environment settings with safe request URL rebasing
- Named request saving, reopening after restart, and unsaved-change protection
- cURL import with live preview and request Code export/copy via GPUI Kit dialogs
- Cross-platform keyboard, selection, and clipboard behavior

## Install

Download the package for your operating system from
[GitHub Releases](https://github.com/847850277/postman-gpui/releases):

| Platform | Desktop package | Supported target |
| --- | --- | --- |
| macOS | Universal `.dmg` or zipped `.app` | Intel and Apple silicon, macOS 10.15.7+ |
| Windows | NSIS installer `.exe` | Windows 10+, x86_64 |
| Linux | `.AppImage` or `.deb` | x86_64, Vulkan-capable Wayland or X11 desktop |

Releases also include standalone `postman-g` CLI archives for macOS, Windows, and Linux.
See [Installation](installation.md) for CLI installation, platform requirements,
unsigned prerelease warnings, and Linux runtime packages. Verify downloads with the release's
`SHA256SUMS` file.

## Build from source

The repository pins Rust in `rust-toolchain.toml`.

```bash
git clone https://github.com/847850277/postman-gpui.git
cd postman-gpui
cargo run --locked
```

Linux needs the GPUI development libraries listed in the
[installation guide](installation.md#linux).

Normal startup opens **Home**, with separate **HTTP** and **Flows** destinations.
Home resumes real requests from the current session; changing pages preserves HTTP
drafts, responses, and running requests. History, search, cookies, and keyboard help
remain available. Flows supports session-only documents as described below.
Unsaved drafts are not restored across application restarts;
saved requests can be reopened from Home or search.

The shell defaults to light appearance; the rail's theme button switches it manually
and restores that choice on restart. Home cards stack at compact window widths;
the native minimum is 768 × 600.

GPUI Kit migration happens directly in the application opened by `cargo run --locked`.
The application and native tests pin GPUI Kit **0.7.3** with its matching
**gpui-pre 0.3.9** snapshot; update these together and regenerate `Cargo.lock`.
History, global search, and keyboard help now use Kit modal dialogs. History groups real
SQLite records by local date, filters method/name/URL/headers/body, and replays the stored
request and response without sending. Each row's trash button deletes only that history entry
after confirmation. Refresh retains selection; **Clear…** requires confirmation and clears all
history, including filtered-out entries. Neither operation deletes saved requests or open drafts.
Search (`Cmd/Ctrl+K`) distinguishes open tabs, saved requests, and history and supports Up/Down and Enter.
`Cmd/Ctrl+Shift+F` opens History; `Cmd/Ctrl+/` opens the native shortcut reference.
Escape closes the topmost dialog and restores focus. In-flight requests continue normally.
Use Home → HTTP to compose requests and inspect the real response. Body offers
Pretty/Raw display and text selection; the copy action preserves the received raw body.
Headers retain duplicate entries, and Cookies/redirects remain inspectable.

HTTP and Flows share an environment selector and **Settings…** dialog built with GPUI Kit.
The first run starts with no environments; **Create environment…** adds a name and an absolute
HTTP(S) base URL, optionally with a path. Names must be unique; URLs cannot contain credentials,
query parameters, or fragments. Cancel discards edits. Save changes and environment switches
commit to the local `postman-gpui/environments.json` configuration before updating matching open
request drafts. Paths, query order, repeated parameters, and other request fields are retained;
unrelated URLs, running request snapshots, and history are unchanged. A failed save retains the
draft for retry; a damaged configuration offers reload without overwriting the file.
Flows shares these settings; environment changes do not rewrite literal URLs in Flow steps. Run
`cargo test --locked --test ui_environments` for the environment interaction and transport tests.

Use the HTTP rail's **Import** action to preview a literal POSIX cURL command and create a new
unsent request tab. **Code** exports the current effective request and copies the displayed
command; **Import cURL** switches directly to the importer. Neither action executes commands
or reads files. See [cURL support and native adaptations](curl.md) for accepted flags,
file-reference export, and redirect/version requirements.

**Save** / **Cmd+S** / **Ctrl+S** stores a named request locally. Reopen it after
closing or restarting through **Home → Saved requests** or global search. Closing
modified requests offers Save, Discard changes, or Cancel; sending does not mark
a document saved. Saved documents retain explicitly entered credentials in
unencrypted local storage. See [saved requests](saved-requests.md) for the
storage contract, file references, recovery and validation.

The request/response split uses columns at an available editor width of 900 logical
pixels and stacks below it. History and search overlay the editor without changing its split.
The response toolbar
can keep a stacked layout explicitly or return to automatic layout. Drag the divider,
double-click to reset to 46%, or focus it and use arrows (2%), Shift+arrows (10%),
Home/End, and Escape to cancel a drag. Enter opens the same **Panel sizes** dialog
as the toolbar button. Each arrangement's ratio and the explicit layout preference
are saved locally; an invalid/unavailable preference file falls back to a usable session.

Params and Headers include editable Description notes, retained in the current tab's draft
and in saved request documents, and excluded from the outgoing request. History replays the effective request; it does not
persist these notes. Wide layouts show a Query String preview and an authorization shortcut
outside the Body editor. Body offers None, JSON, Raw, URL encoded, Form-data and Binary,
retaining each type's draft when switching. Its actions menu contains Request details,
Sample JSON and Clear body. Auth places its scheme selector beside the credential fields,
with both bearer tokens and passwords masked. Request tables, body editors and response
panels use Kit scrollbars for wheel, thumb-drag and track-click scrolling. Search, auth,
Options, descriptions and table cells use retained Kit inputs for selection, IME, undo
and native editing menus. JSON/Raw request bodies, pre-request scripts and tests use
Kit Textarea. Response bodies use a read-only Kit Editor with JSON highlighting, line
numbers in Pretty mode, wrapping in Raw mode, and per-request selection/scroll state.
Kit context menus support editing and copying; the toolbar copies the full raw response.

Run `cargo test --locked --test ui_shell --test ui_kit --test ui_layout --test ui_response_layout --test ui_request_panels` for navigation, in-flight
request preservation, keyboard behavior, and geometry at 960 × 640 through
1920 × 1080 in both themes. `tests/ui_visual_compare.py` compares captured HTML/native
control regions with separate color and text-antialiasing tolerances; its module
docstring describes the comparison manifest. Screenshots belong in PR evidence.

To create native packages locally, install the pinned packager and run the release helper:

```bash
cargo install cargo-packager --version 0.11.8 --locked
python3 scripts/release.py package
```

On macOS, build a universal package with:

```bash
python3 scripts/release.py package --universal-macos
```

## Native Flow editor (P5.2 — Step configuration)

**Flows → New flow** creates an empty session draft. Name validation runs in the Kit dialog;
Cancel/Escape keeps the library unchanged. Rename the active flow in its heading, filter the
library, or resume a real session flow from Home. **Add step → New HTTP request** appends and
selects a new step. The Kit inspector edits its name, method, URL, headers, and body immediately;
the step ID stays stable. Each step retains its own text, errors, selection, and undo history across
step/flow/page switches, theme changes, and resizing.

URL and **Headers · N** offer **Literal / Expression** modes. Literal URLs keep `{{...}}` as text.
Expression mode uses native JSON notation, for example
`{"concat":[{"input":"base_url"},{"literal":"/post"}]}`. Headers preserve ordered duplicates and
empty values; literal lines split at the first colon, while expression mode accepts an ordered
array of `{name, value}` text expressions. Switching back is available only for lossless literal
representations. Available variables show declared input names and exports of preceding steps,
using stable step IDs and never exposing runtime/default values.

**Body expression** accepts the native body JSON object: `none`, `json` (typed JSON expressions),
`json_template`, `raw`, `url_encoded`, or `file` (text expressions). For example,
`{"kind":"json","value":{"literal":{"enabled":true,"count":3}}}` keeps types intact.
Empty source clears a body or headers. File expressions edit only a path expression; no file is read.
Invalid source stays in its editor with a local error and counts as a session edit; the typed document
keeps the last valid field until corrected. Other fields remain editable. API references and loop
request configuration remain read-only; their names can still be edited.

The fixed **Request / Checks / Exports** tabs retain each pane's scroll position; arrows and
Home/End navigate a focused tab. **Checks** edits independent response-status cards (100–599).
Blank clears that check; malformed input keeps its last valid definition. Other native check kinds
are visible as readonly definitions in their original order. Multiple cards extend the HTML preview's
single status card to preserve native documents.

**Exports** edits ordered Name/JSONPath rows with Add/Remove and a native **Sensitive** checkbox.
Names must be nonempty and unique within the step; paths use the engine's subset, including `$`,
`$.json.id`, and `$.items[0].id`. Wildcards and filters are unsupported. Incomplete/invalid rows keep
their exact input and previous valid definition; other rows stay editable. Valid changes refresh
node badges and already-open later-step variable hints without resetting their source or undo.
Renaming/removing exports leaves existing references unchanged. Sensitivity is a definition flag;
editing does not extract or reveal response values.

**Request → Run condition** edits native `when` JSON, for example
`{"eq":[{"output":{"step":"query-status","name":"status"}},{"literal":2}]}`.
All native condition operators and typed operands are preserved; blank removes the condition and
invalid nonempty source retains the last valid value. The editor checks syntax/shape only, without
resolving references, evaluating conditions, or sending requests. API/loop configuration stays readonly.

**Inputs** opens flow-level **Run inputs / Definitions**, including an empty flow. Definitions
supports ordered input declarations, names, optional defaults, and sensitivity. No default means
required; a JSON `null` default is present. Text mode preserves literal strings (including `00123`
and `{{name}}`); JSON mode preserves numbers, booleans, null, arrays, and objects. Switching back
to Text requires a JSON string. Invalid names, duplicate names, and malformed values retain their
drafts and last valid definitions. Rename/remove updates variable-name hints without rewriting
references. Input definitions do not change step requests, checks, conditions, or outputs.

Run values use **explicit override → selected environment base URL (only declared `base_url`)
→ default → missing required input**. Enable Override to supply a session value; Reset removes
it and resumes inheritance. An enabled empty Text value is an empty string; JSON `null`, `false`,
and `0` are present values. Empty/malformed JSON is an error and blocks parameter preparation;
it never silently falls back to a stale value. Environment changes update inherited values while
preserving explicit overrides and unfinished JSON. Run values do not modify document revisions,
defaults, or environment storage, and are not saved across restart.

Sensitive values are concealed until explicitly revealed for editing. Leaving the editing context
conceals them again without discarding the draft. A sensitive **default** still belongs to the
document; the sensitivity flag is a redaction annotation, not encryption. Session overrides are
never written into the document. Input editing prepares parameters only: it does not enable
save/export or Run. Return with Step details or select a canvas node to resume its retained
Request / Checks / Exports state.

Click a node, then use Up/Down or Home/End to navigate the sequential canvas. The library,
inspector, and Console dividers support dragging, arrows, Shift+arrows, Home/End, double-click
reset, and Enter for a numeric size dialog. Escape cancels an active drag. Preferred sizes return
when the window expands; these Flow panel preferences last for the session. At narrow widths,
**Choose flow** replaces the library and **Canvas / Details** switches the editing surface.

The inspector can grow beyond 480 logical pixels while preserving space for the canvas.
The bottom **Console** has Logs, Steps, and Outputs tabs and can collapse to its heading or
maximize within the editor. Restoring it returns to its preferred height. Logs support level
filtering, copying visible entries, clearing, and auto-scroll; scrolling back pauses follow mode.

The fixed `flow ›` input accepts `help`, `clear`, `logs`, `steps`, `outputs`, and `check`. Enter or
Cmd/Ctrl+Enter submits; Up/Down recalls history and restores unfinished input; Tab completes a
unique prefix; Escape clears input. Each flow normally keeps up to 500 log entries and 50 commands
for the session; a complete Check report can exceed the log limit so no diagnostics are lost.
Clearing logs keeps command history and document edits. Commands never invoke a system shell.
`check` validates the current document and pending edits. `run` and `stop` currently report that
their native capability is unavailable; Steps and Outputs remain not run. No requests are sent by
Console commands.

Step details include **Move step up**, **Move step down**, and **Remove step** across all
three tabs. Moves change execution order and keep the selected step, its ID, exact unfinished
fields, and text undo history. Canvas arrow keys still navigate without reordering. First/last
move buttons are disabled at their boundaries.

Remove opens a confirmation naming the step and its ID. Cancel/Escape keeps all edits. Confirming
removes the step (and any contained loop steps), then selects the next or previous step; removing
the last step focuses Add step. Only the deleted step's editors/subscriptions/layout are discarded.
Inputs overrides and Console history are retained. IDs are not recycled within the session.
API-reference and loop nodes move as complete units; their configuration remains read-only.
References are never rewritten: moving a consumer before its producer or removing a producer can
leave invalid references. Variable hints update immediately; use Check to locate compiler diagnostics.
Structural undo/redo and drag-and-drop ordering are not implemented; text undo/redo is preserved.

### Copy HTTP requests into a flow (#228)

Use **Add to flow** beside Code/Save in HTTP. Choose an existing flow or **Create a new flow…**,
enter its name, then **Add and open flow**. Flows → **Add step** → **From open HTTP requests**
uses the same copy operation; **New HTTP request** still creates a blank step. Searchable pickers
identify flows and tabs separately even when their names match. The new step opens in Request
Details, including compact windows.

The dialog captures the current editable draft. If that source changes or closes before confirmation,
cancel and reopen it. Cancel/Escape creates nothing. A confirmed copy is independent: HTTP edits,
unsaved state, undo, responses and any in-flight Send remain intact. Existing Flow editors, Inputs
run overrides and Console history also remain intact. Transfer itself sends no requests.

The effective request follows HTTP Send construction: exact URL/query and ordered enabled headers,
including Basic/Bearer/manual Authorization and automatic Content-Type/Accept. None, JSON, Raw and
URL-encoded bodies retain their kind and bytes. Template-looking text such as `{{token}}` stays
literal. Timeout/redirect/hop settings are explicit; no-timeout remains disabled instead of inheriting
a Flow environment timeout. Binary bodies copy an absolute Unicode file reference without reading
it; relative or non-Unicode paths are rejected. This extends the browser preview, which cannot
preserve native file paths. Form-data, nonempty Scripts/Tests and invalid URLs/headers/JSON are
rejected with inline feedback. Descriptions, disabled/incomplete rows, inactive body drafts and
runtime cookies are editor/session data, retained in HTTP but not copied into the step. New checks,
exports and conditions start empty. This conversion is not a whole-flow Check or Run.

Flow drafts are **not saved across restart**. File Export, Save, and Run remain unavailable.
Request edits preserve conditions, checks/exports, options/auth, API catalogs, nested steps,
inputs/outputs, and layout. Local JSON shape or URL feedback does not compile or execute a flow.
Other check kinds remain readonly. The next P5.3 slice adds file persistence; P6 connects execution.

Run `cargo test --locked -p postman-gpui --lib app::postman_app::flows` and
`cargo test --locked --test ui_flows` for native control, document preservation, and layout checks.

### Check and readonly YAML

Toolbar **Check** and Console `check` compile the same current Flow document, including its API
catalog. They never send requests, read binary files, or resolve environment values. Missing run
inputs and invalid runtime overrides do not block document validation. Unapplied Request, Checks,
Exports, and Inputs definition edits do block it, including drafts in hidden steps or tabs.

The Console shows **Not checked / Check passed / Check failed / Check stale** independently of
execution. Editing a document invalidates its result; navigation, resizing, theme changes, and
runtime overrides do not. Each Check adds a summary and every compiler/draft diagnostic to Logs. Logs retain codes, fields, and step/API locations. Activate **Locate**
with the pointer or keyboard to restore the corresponding editor, including from YAML or a
maximized Console. Nested/API diagnostics reveal their readonly owner; document-level diagnostics
show their location. Old diagnostics require a new Check after edits, so they cannot target a
reordered or deleted row. Command history remains available.

**Canvas / YAML** is also available in compact windows. The retained Kit editor supports selection,
copy, and scrolling, but cannot modify the document. **Copy YAML** copies the exact current
`write_flow_yaml` output, preserving richer DSL fields and layout. It excludes runtime overrides,
resolved environment values, logs, and results. Explicit sensitive defaults are document content
and remain in YAML; the sensitive flag is not encryption.

Compiler-invalid documents can still have valid YAML. Unapplied editor drafts instead show the
**last applied document** with a stale notice and disable Copy YAML; with no prior preview, no
source is shown. Serialization errors also disable Copy. Fix the draft to refresh the preview.
Switching views/flows retains per-Flow editor selection and scroll when the source is unchanged.
Preview and Copy never write a file or mark the session draft saved.

## Flow DSL and CLI

The UI-free runner compiles `.http` files and native `.http.yml` flows to the same `postman-flow`
plan and executes them through the shared `postman-http` / `postman-request` transport.
Unhandled transport, assertion, or loop failures produce a non-zero exit code.

Flow v1 supports ordered HTTP steps, typed inputs and outputs, JSONPath extraction, assertions,
conditional steps, bounded `for_each` / `repeat_until` loops, and reusable API definitions.
Reports preserve JSON types and redact outputs marked sensitive.

Try the bundled API-catalog example from the repository root:

```bash
# Parse and compile without sending requests
cargo run --locked -p postman-cli -- run \
  crates/postman-flow/examples/flows/httpbingo_catalog.http.yml --check

# Execute against HTTPBingo and print a JSON report
cargo run --locked -p postman-cli -- run \
  crates/postman-flow/examples/flows/httpbingo_catalog.http.yml \
  --input client=hello --json
```

With the standalone binary installed, use `postman-g run <file-or-directory>`.
`--input` and `--var` are aliases. Directories are discovered recursively and run in sorted path
order; files in one invocation share a Cookie session.

See the [CLI reference](../crates/postman-cli/README.md), [Flow v1 specification](../crates/postman-flow/README.md),
[JSON Schema](../crates/postman-flow/flow-v1.schema.json), and [example flows](../crates/postman-flow/examples/flows).

## MCP server for Agent-generated flows

`postman-flow-mcp` exposes the Flow v1 schema, reference, validated examples, compiler diagnostics,
inspection, and canonical YAML creation over MCP stdio. Agents submit structured JSON documents;
the server compiles them before writing and confines file access to the configured workspace root.
It does not execute network requests.

```bash
cargo run --locked -p postman-flow-mcp -- --root /absolute/path/to/flow-project
```

See the [MCP server reference](../crates/postman-flow-mcp/README.md) for its six tools and client
configuration.

## HTML UI prototype

Run a preview server from the repository root; no frontend build or package installation is required:

```bash
python3 -m http.server 4173 --bind 127.0.0.1
```

Open the [prototype](http://127.0.0.1:4173/prototypes/request-workspace.html#home).
Home separates HTTP requests from Flows. The responsive layout defaults to light mode and offers
a dark-mode switch. The HTTP screen includes cURL import/export and environment settings; the
Flow screen includes step editing, checks, preview results, and `.http.yml` export.

Send and Run preview use local fixtures. Flow checks are partial browser-side checks, and editor
data lasts only for the page session. The prototype is not connected to the Rust runtime and does
not replace native-client E2E tests. See the [prototype guide](../prototypes/README.md) for interactions,
supported syntax, and limitations.

## Verify

```bash
cargo fmt -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
python3 -m unittest discover -s scripts/tests
```

Run the public HTTPBingo scenarios separately; they require network access:

```bash
cargo httpbingo-scenarios
```

The HTML parser/model tests require Node.js 18 or later:

```bash
node --test prototypes/curl-request.test.cjs prototypes/flow-model.test.cjs
```

See the [request scenario guide](../tests/cases/README.md) for desktop coverage and the
[Flow E2E guide](../crates/postman-flow-e2e/README.md) for service prerequisites and dedicated
Vaultwarden, Meilisearch, and Qdrant suites. Service-dependent tests need their documented setup;
a normal workspace test run alone does not establish that these suites ran.

## Current limitations

- Response bodies use a buffered text model. A byte-native response viewer, atomic response
  save-as, and incremental streaming-response progress are not yet implemented.
- The native Flow editor supports session Request drafts; use the CLI for actual Flow execution.
- Request saving is explicit; unsaved drafts are not recovered after a crash or force quit.
- Native cURL import supports the documented inline subset; file-based and multipart commands
  are export-only.

## Documentation

- [Changelog](../CHANGELOG.md) and [open issues](https://github.com/847850277/postman-gpui/issues)
- [cURL import/export support](curl.md)
- [Editor state and synchronization](editor-state.md)
- [Release runbook](releasing.md)
- [Cross-platform smoke checklist](release-smoke-test.md)

## Local data and privacy

Completed request history is stored in `postman-gpui/request-history.sqlite3` under the operating
system's local application-data directory. Known credentials and cookies are removed before
persistence. Cancelled requests and transport failures are not recorded.

## License

[MIT](../LICENSE)
