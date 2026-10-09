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

Try:

- Start at Home and choose HTTP requests or Flows. The left navigation preserves
  both editors' session state when switching. `#home`, `#http`, and `#flows` are
  direct links; browser Back/Forward follows navigation between modes.
- Open History from the left navigation to filter or replay recent requests.
- Change query parameters and watch the URL update.
- Inspect the query string summary and open its authorization settings.
- Switch requests and the Params, Headers, Body, and Auth panes.
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
- Send or cancel; Create user returns 201, Missing endpoint returns 404.
- Filter or replay history, or search with Cmd/Ctrl+K.
- Switch Pretty, Raw, and response Headers, and copy a response.
- Open Code to generate and copy cURL from the current request configuration.
- Open Import in the left navigation, or Import cURL in the Code dialog. Paste a
  command, review its method, URL, headers, body, and authorization summary, then
  import it into a new tab. Cmd/Ctrl+Enter also imports a valid command.
- Edit imported JSON or raw text in Body. Code exports the edited request back
  to cURL, including explicit empty bodies and bodies on custom HTTP methods.
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

- Choose User onboarding (three HTTP steps, output references, and a condition)
  or Service readiness (bounded polling). Create a new flow using the plus next
  to Your flows; in compact windows use the Choose flow menu.
- Select a node to edit its request, checks, and exports in the right inspector.
  Narrow windows switch between Canvas and Details. Move steps using the up/down
  buttons; Check reports references made invalid by changing execution order.
- Add a blank step or copy an open HTTP request. The HTTP editor's Add to flow
  action also copies a request into an existing or new flow. Unsupported cURL
  transport flags, header suppression, and nonstandard HTTP methods are rejected
  during conversion instead of being silently discarded.
- Use Inputs for per-run values and the shared environment picker for `base_url`.
  These runtime overrides are kept separate from YAML defaults.
- Switch Canvas / YAML to inspect generated Flow v1 source, then copy it or
  Export a `.http.yml` file. YAML is read-only in this prototype; structured
  body/condition expressions can be edited in the inspector. Invalid JSON drafts
  survive navigation and must be fixed before running or exporting.
- Run preview (Cmd/Ctrl+Enter) shows sequential step results, checks, extracted
  outputs, failures, and cancellation. Cmd/Ctrl+S exports the active flow.
- Resize the flow library, step details, and run results using the same dividers.
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
`panel-resize.css` and `panel-resize.js` share the split handles and accessible
size controls between HTTP and Flows.
`request-tabs.css` and `request-tabs.js` handle the responsive tab rows, fixed
tools, active-tab visibility, and searchable list of open HTTP requests.

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

Unsupported options, file references, multipart uploads, shell expansion, and
multiple URLs produce inline errors and disable import. Commands are parsed as
text, never executed. Cancel and Escape leave existing requests unchanged.
PowerShell/CMD syntax is not supported. Output-only flags such as `--silent` and
`--verbose` do not change the imported request and are omitted from exports.

The parser and exporter share `curl-request.js`. Run their dependency-free tests
with Node.js 18 or later:

```sh
node --test prototypes/curl-request.test.cjs prototypes/flow-model.test.cjs
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
