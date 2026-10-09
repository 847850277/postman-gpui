# Request workspace prototype

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

- Change query parameters and watch the URL update.
- Switch requests and the Params, Headers, Body, and Auth panes.
- Send or cancel; Create user returns 201, Missing endpoint returns 404.
- Filter or replay history, or search with Cmd/Ctrl+K.
- Switch Pretty, Raw, and response Headers, and copy a response.
- Toggle light/dark colors and stacked/side-by-side panels.

The desktop reference viewport is 1440 × 960 CSS pixels at 100% browser zoom.
CSS variables define colors, typography, and shell dimensions. These are
proposed design values, not assertions of parity with the current client.
