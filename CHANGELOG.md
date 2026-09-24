# Changelog

All notable changes to Postman GPUI are documented in this file. The project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- `BodyInput::form_data_entry_count` now reads the form editor's rows directly instead of
  maintaining a duplicate count. This changes the public Rust signature from
  `form_data_entry_count(&self)` to `form_data_entry_count(&self, cx: &gpui::App)`.
  Rust callers must migrate `input.form_data_entry_count()` to
  `input.form_data_entry_count(cx)` using their current GPUI application context; this is
  a source-breaking change for consumers of the UI library. Both workspace callers have
  been migrated. Disabled and blank editor rows still count, and application behavior
  and persisted data formats are unchanged. Refs #190, #180.

### Planned

- Byte-native downloads, atomic save-as, and streaming progress/cancellation remain pending in
  [#69](https://github.com/847850277/postman-gpui/issues/69).

## [0.1.0] - Unreleased

### Added

- Native GPUI request workspace with multi-tab isolation and keyboard-first editing.
- HTTP methods, query parameters, custom headers, cookies, Basic/Bearer authorization, and request
  bodies including JSON, raw, URL-encoded, and multipart file upload.
- Redirect policy, response decompression, timeout, cancellation, status/header inspection, body
  formatting, and quick copy.
- Local SQLite request history with redaction, persistence recovery, search, and complete-request
  replay.
- HTTPBingo application E2E scenarios and deterministic local request/UI regression suites.
- Embedded static Inter UI weights and JetBrains Mono for consistent rendering on clean macOS,
  Windows, and Linux installations.
- Reproducible macOS, Windows, and Linux packaging and tag-driven GitHub Releases.

### Known limitations

- Binary and range responses do not yet have a byte-native viewer.
- Large responses cannot yet be saved atomically to disk.
- Streaming responses do not yet expose incremental progress or stream cancellation semantics.
- Unsigned release candidates can trigger macOS Gatekeeper or Windows SmartScreen warnings. Final
  release packages should be signed using the credentials documented in `docs/releasing.md`.

[Unreleased]: https://github.com/847850277/postman-gpui/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/847850277/postman-gpui/releases/tag/v0.1.0
