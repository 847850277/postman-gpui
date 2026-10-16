# HTTP and Flows prototype

The current design is a desktop API client with white surfaces by default,
an orange primary action, an optional dark theme, and separate colors for HTTP
methods and response status. Home separates HTTP requests from Flows before
entering either editor. It uses the Developer Tool / IDE guidance from `ui-ux-pro-max`, adapted
to this app's existing local Inter and JetBrains Mono fonts.

Open `request-workspace.html` in a browser. It uses the fonts already bundled in
`../assets/fonts/` and needs no build step, package installation, or CDN.

For a local preview with clipboard access, run from the repository root:

```sh
python3 -m http.server 4173 --bind 127.0.0.1
```

Then visit <http://127.0.0.1:4173/prototypes/request-workspace.html>.

This is an exploratory HTML prototype, not a GPUI implementation or an E2E
baseline. Requests use local fixtures; clicking Send does not contact the URL.
Edits and saved requests last for the current page session only.

All shipped HTTP requests, cURL examples, and Flow defaults use
[HTTPBingo](https://httpbingo.org/). The sample environment is HTTPBingo; add
other environments in Settings as needed. The local fixtures cover the shipped
query, header, UUID, JSON echo, and status examples. They preserve HTTPBingo's
array-valued query parameters and JSON echo paths, but are not a full service
emulator. UUID, origin, and timing are fixed preview values; file bodies show
metadata placeholders. HTTPBingo echoes data rather than storing users or orders.

Try:

- Start at Home and choose HTTP requests or Flows. The left navigation preserves
  both editors' session state when switching. `#home`, `#http`, and `#flows` are
  direct links; browser Back/Forward follows navigation between modes.
- Open History from the left navigation to filter or replay recent requests.
- Change query parameters and watch the URL update.
- Inspect the query string summary and open its authorization settings.
- Switch requests and the Params, Headers, Body, and Auth panes.
- Body has six modes: None, JSON, Raw, URL encoded, Form-data, and Binary.
  Raw offers Text, XML, HTML, and JavaScript content types. Each mode retains
  its own draft when switching; requests and history replays have independent
  drafts. None excludes the body even on POST/PUT/PATCH requests.
- Add, disable, or remove form fields; URL encoded preserves repeated keys and
  shows the encoded payload. Form-data combines Text and File fields. Binary
  selects one file, with a drop target and replace/remove actions. Narrow panes
  stack key/value controls without horizontal scrolling.
- Content-Type follows the body automatically unless an enabled header overrides
  it. The automatic header is shown under Headers; multipart boundaries are left
  to the sender. JSON errors, missing keys, and missing files appear inline and
  focus the relevant control when Send or Code is invoked.
- Open many requests: tabs adapt to the available width and wrap onto additional
  rows in their original order. New and the open-request count stay at the top
  right. The tab area uses up to roughly one third of the window height (at most
  eight rows); larger sets scroll vertically within that area. There is no
  horizontal scrolling. Creating or selecting a request brings its row into
  view, including after the window is resized.
- Click the open-request count to search all tabs by name, method, or URL. The
  list marks the current request and unsaved edits; Up/Down moves between items,
  Enter opens one, and Escape returns to the tab bar. New untitled requests get
  sequential names so multiple blank tabs remain distinguishable.
- Send or cancel; Echo JSON returns 200, and Not found status returns an empty 404 response.
- Filter or replay history, or search with Cmd/Ctrl+K.
- Switch Pretty, Raw, and response Headers, and copy a response.
- Open Code to generate and copy cURL from the current request configuration.
- Open Import in the left navigation, or Import cURL in the Code dialog. Paste a
  command, review its method, URL, headers, body, and authorization summary, then
  import it into a new tab. Cmd/Ctrl+Enter also imports a valid command.
- Edit imported JSON or raw text in Body. Code exports the edited request back
  to cURL, including explicit empty bodies and bodies on custom HTTP methods.
  Form-data exports `--form-string` for literal text and `--form` for files;
  Binary exports `--data-binary`. File references use filenames relative to the
  terminal's current directory, with a reminder in Code to adjust paths.
  The browser retains file metadata only: this prototype neither reads file
  contents nor uploads files. Send still returns a local fixture.
- Use Settings next to the environment picker, or its Manage environments item,
  to add, rename, remove, and configure environments. Save applies the changes;
  Cancel or Escape discards them. Invalid names and URLs show inline errors.
- Toggle light/dark colors and stacked/side-by-side panels.
- Drag the divider to resize request/response widths or stacked heights. Hover
  highlights its grip; double-click restores the default 46/54 split. Click the
  divider for numeric size controls, adjustment buttons, and Reset to default.
  Keyboard users can Tab to a divider, use arrow keys (Shift for larger steps),
  Home/End for its limits, or Enter for size controls. Escape cancels a drag.

In Flows:

- Choose UUID and JSON echo (three HTTP steps, output references, and a condition)
  or Poll echoed status (bounded polling that succeeds on its first ready response). Create a new flow using the plus next
  to Your flows; in compact windows use the Choose flow menu.
- Select a node to edit its request, checks, and exports in the right inspector.
  Narrow windows switch between Canvas and Details. Move steps using the up/down
  buttons; Check reports references made invalid by changing execution order.
- Add a blank step or copy an open HTTP request. The HTTP editor's Add to flow
  action also copies a request into an existing or new flow. Unsupported cURL
  transport flags, header suppression, and nonstandard HTTP methods are rejected
  during conversion instead of being silently discarded.
  URL encoded bodies transfer as encoded raw text with their Content-Type.
  Form-data and Binary conversion is not implemented in the Flow preview and
  reports an explicit error rather than silently discarding the file fields.
- Use Inputs for per-run values and the shared environment picker for `base_url`.
  These runtime overrides are kept separate from YAML defaults.
- Switch Canvas / YAML to inspect generated Flow v1 source, then copy it or
  Export a `.http.yml` file. YAML is read-only in this prototype; structured
  body/condition expressions can be edited in the inspector. Invalid JSON drafts
  survive navigation and must be fixed before running or exporting.
- Run preview (Cmd/Ctrl+Enter) shows sequential step results, checks, extracted
  outputs, failures, and cancellation. Cmd/Ctrl+S exports the active flow.
- The bottom **Console** has Logs, Steps, and Outputs tabs. Logs show timestamps,
  levels, requests, checks, polling, and cancellation; filter by level, copy the
  visible view, or clear logs without removing step results and outputs. Click a
  step name to locate it on the canvas and in the inspector. Auto-scroll follows
  new events and pauses when you scroll back; enable it again to return to the
  latest event. Logs retain up to 500 events per flow, and a new run starts a new
  log. All events describe local fixtures, not network execution.
- The fixed `flow ›` command line accepts `help`, `check`, `run`, `stop`, `clear`,
  `logs`, `steps`, and `outputs`. Enter executes; Up/Down recalls each flow's last
  50 commands and restores unfinished input; Tab completes an unambiguous command;
  Escape clears input. Command drafts survive switching flows. A second `run`
  does not cancel an active preview: use `stop`. Commands and feedback appear in
  Logs, and `clear` keeps results, outputs, and command history. These are Flow
  preview commands, not a system shell; arbitrary commands are rejected.
- Collapse Console to its title bar, maximize it within the editor, or restore
  its previous height. Tabs support Left/Right and Home/End. Selecting a tab or
  starting a preview reopens a collapsed Console; locating a step restores the
  canvas when Console is maximized.
- Resize the flow library, step details, and Console using the same dividers.
  Step details can grow beyond 480px, keeping at least 300px for the canvas.
  Short windows keep Console and its actions visible while panels scroll inside.
  Library/detail dividers disappear when compact navigation replaces their
  panels, and the details divider is hidden in YAML view. The icon navigation
  rail remains fixed.

The Flow screen is an interactive design prototype, not a connection to the Rust
runtime. Run preview uses local fixtures and a small expression interpreter;
polling waits are shortened to at most 1.5 seconds. Check performs partial local
checks, not full engine compilation. Arbitrary YAML import/editing, free-position
node dragging, general loop creation, and production execution are outside this
prototype. Exported example documents have been checked using the existing
`postman-g run <file> --check` command without sending network requests.

`flow-studio.css` and `flow-studio.js` implement the new screens;
`flow-model.js` holds sample definitions, conversion, and preview helpers.
`httpbingo-demo.js` shares the local HTTPBingo response fixtures between both editors.
`panel-resize.css` and `panel-resize.js` share the split handles and accessible
size controls between HTTP and Flows.
`request-tabs.css` and `request-tabs.js` handle the responsive tab rows, fixed
tools, active-tab visibility, and searchable list of open HTTP requests.
`body-editor.css` and `body-editor.js` implement the body controls;
`request-body.js` owns type-specific drafts, validation, headers, and payloads.

Environment base URLs support HTTP(S), ports, and path prefixes. Switching or
editing the active environment rebases matching requests and preserves endpoint
paths and query parameters; unrelated URLs stay as entered. Environment settings
also last only for the current preview session.

cURL import accepts one HTTP(S) request using Bash/POSIX quoting and backslash
line continuations. It supports `-X`, `-H`, inline `--data`, `--data-raw`,
`--data-binary`, `--data-urlencode`, `--json`, `-G`, `-I`, Basic/Bearer auth,
inline cookies, user agent, and referer. Query parameters and duplicate headers
are retained. Bearer authorization is editable under Auth; Basic and other
explicit authorization headers are editable under Headers. Redirect,
compression, TLS verification, and URL globbing flags are shown in the import
preview and retained when exporting; Send still uses local fixtures.

During cURL import, unsupported options, file references, multipart uploads,
shell expansion, and multiple URLs produce inline errors and disable import.
File and multipart requests can be composed in Body and exported, but cannot
yet be imported back. Commands are parsed as
text, never executed. Cancel and Escape leave existing requests unchanged.
PowerShell/CMD syntax is not supported. Output-only flags such as `--silent` and
`--verbose` do not change the imported request and are omitted from exports.

The parser and exporter share `curl-request.js`. Run their dependency-free tests
with Node.js 18 or later:

```sh
node --test prototypes/*.test.cjs
```

The desktop reference viewport is 1440 × 960 CSS pixels at 100% browser zoom.
Light mode is the default; the theme button switches to dark mode. The request editor fills the viewport without a permanent
sidebar. At editor widths of 900px or more, request and response panels sit
side by side; narrower windows stack them automatically. A ResizeObserver keeps
the layout control in sync, and a manually selected stacked layout is retained
when resizing. Panel heights share the available space. Short windows use more
compact headers and scroll within the editor; the status bar stays visible.

Panel sizes are the exception to session-only prototype state: layout preferences
are saved in this browser's local storage. HTTP keeps separate preferred ratios
for horizontal and vertical arrangements; Flows keeps library/detail widths and
results height. Smaller windows clamp the displayed sizes without overwriting
the preferred values, so expanding the window restores them. Each divider resets
only its own current layout. If storage is unavailable, resizing still works for
the current page. Request data, Flow documents, and environment settings are not
written by this layout preference store.

The `:root` and `[data-theme="dark"]` CSS variables define semantic colors,
typography, shell dimensions, and feedback timing. At the reference viewport,
the rail is 72px, titlebar 52px, request tabs 46px, and status bar 30px. The
request panel takes 46% of the space beside the divider. These dimensions adapt
to smaller widths and heights. These are proposed design values, not
assertions of parity with the current GPUI client.

Controls have accessible names and visible keyboard focus. Request tabs support
Left/Right and Home/End; HTTP request tabs also support Up/Down between rows.
Dialogs support Escape. Reduced-motion preferences
disable transitions. The preview still uses local fixtures and session-only
state, so it does not replace end-to-end testing of the native client.

### Inputs reference update (P5.2-3 / #224)

This change extends design baseline `1c10653` using implementation baseline #223 / `bf18919`.
The implementation PR's commit containing `flow-inputs.js`, `flow-inputs-panel.js`, and the
Inputs styles is the updated reference for this surface. Other Flow surfaces retain that baseline.

Inputs opens Run inputs; Definitions adds names, optional Text/JSON defaults, and sensitivity.
Runtime precedence is explicit override, environment for declared base_url, default, then missing.
Reset removes the override. Null/false/zero/empty string remain explicit values; malformed JSON
blocks preview preparation. Stable rows preserve declaration/override ownership through renames;
references remain unchanged. Sensitive values are concealed until revealed, with session-only
runtime data. The native application uses the same presentation and native codecs; the browser
preview continues using local fixtures rather than real network execution.

`node --test prototypes/flow-inputs.test.cjs` checks precedence, typed values, malformed drafts,
name collisions, rename/delete/recreate behavior, and safe handling of prototype-like names.

### Step management reference update (P5.2-4 / #226)

This extends the merged #225 / `b3d1c1a` baseline. The implementation PR's changes to
`flow-studio.js`, `flow-studio.css`, and `flow-model.js` are the updated step-action reference.
The compact action bar stays below independently scrolling fields across Request/Checks/Exports.
Up/down changes execution order and keeps the selected ID; first/last boundaries disable the
corresponding action. Tooltips explain that existing references remain unchanged.

Remove opens a dialog showing the step name/ID and the effect on unfinished edits and references.
Cancel/Escape preserves the document; confirmation selects next/previous, or Add step when empty.
Loop removal includes nested steps; new IDs do not reuse deleted/imported IDs within the session.
Inputs overrides and Console history stay intact. The native controls use GPUI Kit; the browser
continues using fixture execution. Run `node --test prototypes/*.test.cjs` and manually verify
dialog focus, keyboard activation, cancel/confirm, empty state, and light/dark compact layouts.

### HTTP transfer reference update (P5.2-5 / #228)

Add to flow and Add step share one confirmation dialog. The latter includes New HTTP request and
an open-request selector; selecting a request reads an independent snapshot without activating its
HTTP tab. A new destination gets an inline name field with the existing single-line/80-character
rule. Cancel/failed validation creates no flow. Source revisions are checked again on confirmation;
changed/closed sources require reopening. Success selects the appended node, opens Request Details
in compact layouts and focuses its name. The source summary omits URL/query/header/body values.

The native implementation follows `RequestDraft::construct`, not this preview converter: it retains
URL-encoded BodyTemplate and exact bytes, Basic/Bearer/manual auth precedence, explicit native request
options, and absolute Unicode `BodyTemplate::File` references without reading files. The browser
still rejects Binary (only metadata is available), multipart and generic cURL flags. These are
intentional preview limitations, not additional native restrictions. Native Scripts/Tests are also
explicitly rejected; copying does not enable P5.3 Check/persistence or P6 execution.
