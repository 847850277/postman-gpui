# Request editor prototype

The current design is a desktop API client with neutral charcoal surfaces,
an orange primary action, and separate colors for HTTP methods and response
status. Requests, history, and environments are the main UI concepts. It uses the Developer Tool / IDE guidance from `ui-ux-pro-max`, adapted
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

- Open History from the left navigation to filter or replay recent requests.
- Change query parameters and watch the URL update.
- Inspect the query string summary and open its authorization settings.
- Switch requests and the Params, Headers, Body, and Auth panes.
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
node --test prototypes/curl-request.test.cjs
```

The desktop reference viewport is 1440 × 960 CSS pixels at 100% browser zoom.
Light mode is the default; the theme button switches to dark mode. The request editor fills the viewport without a permanent
sidebar. At editor widths of 900px or more, request and response panels sit
side by side; narrower windows stack them automatically. A ResizeObserver keeps
the layout control in sync, and a manually selected stacked layout is retained
when resizing. Panel heights share the available space. Short windows use more
compact headers and scroll within the editor; the status bar stays visible.

The `:root` and `[data-theme="dark"]` CSS variables define semantic colors,
typography, shell dimensions, and feedback timing. At the reference viewport,
the rail is 72px, titlebar 52px, request tabs 46px, and status bar 30px. The
request panel takes 46% of the space beside the divider. These dimensions adapt
to smaller widths and heights. These are proposed design values, not
assertions of parity with the current GPUI client.

Controls have accessible names and visible keyboard focus. Request tabs support
Left/Right and Home/End; dialogs support Escape. Reduced-motion preferences
disable transitions. The preview still uses local fixtures and session-only
state, so it does not replace end-to-end testing of the native client.
