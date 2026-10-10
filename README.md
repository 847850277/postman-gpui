# Postman GPUI

[![Downloads](https://img.shields.io/github/downloads/847850277/postman-gpui/total)](https://github.com/847850277/postman-gpui/releases)

Postman GPUI combines a native, cross-platform HTTP client built with Rust and GPUI with a
Flow DSL for repeatable HTTP workflows. Use the desktop app for interactive requests, or
`postman-g` to check and execute request files and flows from the command line.

[中文说明](README-zh.md)

![Postman GPUI native desktop client](image.png)

## Project components

| Component | Current capabilities | Entry point |
| --- | --- | --- |
| Native desktop app | HTTP request editing, response inspection, tabs, and local history | `cargo run --locked` |
| Flow DSL and CLI | Compile and execute `.http` requests and `.http.yml` workflows | [postman-g](crates/postman-cli/README.md) |
| Flow MCP server | Schema, examples, validation, inspection, and YAML generation for agents | [postman-flow-mcp](crates/postman-flow-mcp/README.md) |
| HTML UI prototype | Separate Home, HTTP, and Flows screens with simulated responses | [Preview guide](prototypes/README.md) |

The native app now opens Home with separate HTTP and Flows destinations. The visual flow
editor is still an HTML prototype; executable workflows use the CLI or Rust API.

## Desktop HTTP features

- GET, POST, PUT, PATCH, DELETE, HEAD, and OPTIONS requests
- Query parameters, custom headers, Basic/Bearer authorization, and cookies
- JSON, raw, URL-encoded, and multipart request bodies with file upload
- Redirect policy, response decompression, timeout, and cancellation controls
- Response status, headers, formatted body, and quick copy
- Multi-tab requests, global search, and replayable SQLite history
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
See [Installation](docs/installation.md) for CLI installation, platform requirements,
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
[installation guide](docs/installation.md#linux).

Normal startup opens **Home**, with separate **HTTP** and **Flows** destinations.
Home resumes real requests from the current session; changing pages preserves HTTP
drafts, responses, and running requests. History, search, cookies, and keyboard help
remain available. The Flows page is an explicit unavailable state until its editor
and execution UI land. Drafts are not restored across application restarts.

The shell defaults to light appearance; the rail's theme button switches it manually
and restores that choice on restart. Home cards stack at compact window widths;
the native minimum remains 960 × 640.

GPUI Kit migration happens directly in the application opened by `cargo run --locked`.
Use Home → HTTP to compose requests and inspect the real response. Body offers
Pretty/Raw display and text selection; the copy action preserves the received raw body.
Headers retain duplicate entries, and Cookies/redirects remain inspectable.

The request/response split uses columns at an available editor width of 900 logical
pixels and stacks below it (including space taken by History). The response toolbar
can keep a stacked layout explicitly or return to automatic layout. Drag the divider,
double-click to reset to 46%, or focus it and use arrows (2%), Shift+arrows (10%),
Home/End, and Escape to cancel a drag. Enter opens the same **Panel sizes** dialog
as the toolbar button. Each arrangement's ratio and the explicit layout preference
are saved locally; an invalid/unavailable preference file falls back to a usable session.

Params and Headers include editable Description notes, retained in the current tab's draft
and excluded from the outgoing request. History replays the effective request; it does not
persist these notes. Wide layouts share a Query String preview and an authorization shortcut
across request panes. Body formats use a Kit dropdown; **Details** expands the effective
request preview, and the actions menu contains Sample JSON and Clear body. Auth places its
scheme selector beside the credential fields, with both bearer tokens and passwords masked.

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

See the [CLI reference](crates/postman-cli/README.md), [Flow v1 specification](crates/postman-flow/README.md),
[JSON Schema](crates/postman-flow/flow-v1.schema.json), and [example flows](crates/postman-flow/examples/flows).

## MCP server for Agent-generated flows

`postman-flow-mcp` exposes the Flow v1 schema, reference, validated examples, compiler diagnostics,
inspection, and canonical YAML creation over MCP stdio. Agents submit structured JSON documents;
the server compiles them before writing and confines file access to the configured workspace root.
It does not execute network requests.

```bash
cargo run --locked -p postman-flow-mcp -- --root /absolute/path/to/flow-project
```

See the [MCP server reference](crates/postman-flow-mcp/README.md) for its six tools and client
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
not replace native-client E2E tests. See the [prototype guide](prototypes/README.md) for interactions,
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

See the [request scenario guide](tests/cases/README.md) for desktop coverage and the
[Flow E2E guide](crates/postman-flow-e2e/README.md) for service prerequisites and dedicated
Vaultwarden, Meilisearch, and Qdrant suites. Service-dependent tests need their documented setup;
a normal workspace test run alone does not establish that these suites ran.

## Current limitations

- Response bodies use a buffered text model. A byte-native response viewer, atomic response
  save-as, and incremental streaming-response progress are not yet implemented.
- The visual Flow editor, cURL import/export, and redesigned HTTP screen described above are
  prototype features; see the CLI for actual Flow execution.

## Documentation

- [Changelog](CHANGELOG.md) and [open issues](https://github.com/847850277/postman-gpui/issues)
- [Live editor synchronization audit](docs/autofill-contract.md)
- [Release runbook](docs/releasing.md)
- [Cross-platform smoke checklist](docs/release-smoke-test.md)

## Local data and privacy

Completed request history is stored in `postman-gpui/request-history.sqlite3` under the operating
system's local application-data directory. Known credentials and cookies are removed before
persistence. Cancelled requests and transport failures are not recorded.

## License

[MIT](LICENSE)
