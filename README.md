# Postman GPUI

[![Downloads](https://img.shields.io/github/downloads/847850277/postman-gpui/total)](https://github.com/847850277/postman-gpui/releases)

Postman GPUI combines a native, cross-platform HTTP client built with Rust and GPUI with a
Flow DSL for repeatable HTTP workflows. Use the desktop app for interactive requests, or
`postman-g` to check and execute request files and flows from the command line.

[Download for macOS, Windows & Linux](https://github.com/847850277/postman-gpui/releases) · [Installation](docs/installation.md) · [中文](README-zh.md)

<picture>
  <source media="(prefers-reduced-motion: reduce)" srcset="assets/readme/tour-en-static.svg">
  <img src="assets/readme/tour-en.svg" alt="Interface tour: Home, HTTP, Body, History, Search, Environments, cURL, Save, Cookies, Shortcuts, layouts and Flows">
</picture>

Illustrated with sample data. Current source supports Flow files, Check/YAML, Run/Cancel and detailed results.
Published builds can lag behind these capabilities; check the selected release's tag, notes and build metadata.
See the [release runbook](docs/releasing.md) for candidate verification.

[User & development guide](docs/guide.md) · [Flow CLI](crates/postman-cli/README.md) · [MCP](crates/postman-flow-mcp/README.md) · [Changelog](CHANGELOG.md) · [MIT license](LICENSE)
