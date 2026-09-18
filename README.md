# Postman GPUI

[![Downloads](https://img.shields.io/github/downloads/847850277/postman-gpui/total)](https://github.com/847850277/postman-gpui/releases)

Postman GPUI is a native, cross-platform HTTP client built with Rust and GPUI. It focuses on a
fast request/response workflow, keyboard-friendly editing, and local-first request history.

[中文说明](README-zh.md)

![Postman GPUI](image.png)

## Features

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

| Platform | First-release package | Supported target |
| --- | --- | --- |
| macOS | Universal `.dmg` or zipped `.app` | Intel and Apple silicon, macOS 10.15.7+ |
| Windows | NSIS installer `.exe` | Windows 10+, x86_64 |
| Linux | `.AppImage` or `.deb` | x86_64, Vulkan-capable Wayland or X11 desktop |

See [Installation](docs/installation.md) for platform requirements, unsigned prerelease warnings,
and Linux runtime packages.

## Build from source

The repository pins Rust in `rust-toolchain.toml`.

```bash
git clone https://github.com/847850277/postman-gpui.git
cd postman-gpui
cargo run --locked
```

Linux needs the GPUI development libraries listed in the
[installation guide](docs/installation.md#linux).

To create native packages locally, install the pinned packager and run the release helper:

```bash
cargo install cargo-packager --version 0.11.8 --locked
python3 scripts/release.py package
```

On macOS, build a universal package with:

```bash
python3 scripts/release.py package --universal-macos
```

## Headless `.http` / `.http.yml` runner

The UI-free runner compiles `.http` files and native `.http.yml` flows to the same `postman-flow`
plan, executes them through the desktop application's `postman-http` / `postman-request` transport,
and exits non-zero when transport or declarative assertions fail.

```bash
cargo httpbingo-headless
cargo run --locked -p postman-cli -- run path/to/api.http --var host=https://example.com
cargo run --locked -p postman-cli -- run path/to/flow.http.yml --input client=hello
cargo run --locked -p postman-cli -- run path/to/smoke.http path/to/regression/
```

See the [headless runner reference](crates/postman-cli/README.md) for supported syntax, captures,
assertions, JSON reports, per-request timeouts/redirect options, and the 58-family HTTPBingo
coverage matrix.

## Verify

```bash
cargo fmt -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
cargo httpbingo-scenarios
python3 -m unittest discover -s scripts/tests
```

## Release scope

The v0.1.0 scope is tracked by
[#50](https://github.com/847850277/postman-gpui/issues/50). Binary downloads, file-backed response
saving, and streaming response progress remain intentionally pending in
[#69](https://github.com/847850277/postman-gpui/issues/69). The initial reusable HTTP core and
headless `.http` runner are tracked by
[#171](https://github.com/847850277/postman-gpui/issues/171).

See [CHANGELOG.md](CHANGELOG.md), the [live editor synchronization audit](docs/autofill-contract.md),
the [release runbook](docs/releasing.md), and the
[cross-platform smoke checklist](docs/release-smoke-test.md).

## Local data and privacy

Completed request history is stored in `postman-gpui/request-history.sqlite3` under the operating
system's local application-data directory. Known credentials and cookies are removed before
persistence. Cancelled requests and transport failures are not recorded.

## License

[MIT](LICENSE)
