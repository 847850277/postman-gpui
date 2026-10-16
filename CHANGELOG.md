# Changelog

All notable changes to Postman GPUI are documented in this file. The project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- Preserve response bytes before character decoding so binary and non-UTF-8 response sizes
  remain accurate in the response panel and History. Binary responses show a placeholder;
  Save as writes the original body bytes, including when the active response changes.
- Keep the full OPTIONS method visible in Flow request selectors and their dropdown menus.

### Added

- GPUI Kit HTTP Add to flow and Add step from open requests, with searchable destinations,
  inline new-flow naming, immutable source validation, and independent step copies. Conversion
  preserves effective request bytes, auth and options, including native file references; unsupported
  multipart/scripts fail before mutation. Compact layouts open Request Details after insertion.

- GPUI Kit Flow step ordering and removal: adjacent up/down controls, explicit removal
  confirmation, neighbor selection and focus restoration, and non-recycled session step IDs.
  Surviving editors retain drafts and undo; variable hints refresh without rewriting references.
  The HTML prototype includes the matching confirmation and compact inspector action bar.

- GPUI Kit Flow Inputs definitions and session run parameters: typed defaults, sensitivity,
  explicit override/reset, environment inheritance, retained invalid drafts, and live variable
  hints. Runtime edits remain separate from the document; parameter preparation blocks invalid
  values. The HTML Inputs prototype now includes both definition and run-parameter views.

- GPUI Kit Flow Checks/Exports tabs and a retained Run condition editor. Edit ordered status
  checks, export names/JSONPaths/sensitivity, and native conditions with local validation and
  last-valid definitions. Stable row identities preserve unfinished text and undo; valid export
  changes refresh cached later-step hints without rewriting references. Other check kinds and
  API/loop configuration remain readonly. Inspector tabs stay visible while content scrolls.

- GPUI Kit Flow Request details with literal/expression URLs, collapsible ordered headers,
  available-variable hints, and native body JSON editing. Per-step buffers retain invalid drafts,
  local errors, focus, and undo while the document keeps the last valid field.
- Shared field-expression codecs reuse Flow v1 wire types, including strict unknown-field
  rejection for explicit `none` bodies.

- Native session-only Flow library, sequential canvas, and basic HTTP step inspector using
  GPUI Kit. Create, rename, filter, append steps, and resume drafts from Home without losing edits.
- Responsive Flow library/inspector/results panels with pointer and keyboard resizing. Compact
  windows use a flow picker and Canvas/Details switching; the minimum window is now 768 × 600.
- Native Flow Console with Logs/Steps/Outputs, bounded per-flow command history, level filtering,
  copy/clear, auto-scroll, and collapse/maximize/restore. Local commands use retained GPUI Kit
  controls; compilation and execution commands report their unavailable state.
- Flow inspectors can expand beyond the former 480px cap while keeping the canvas usable.

### Limitations

- Flow drafts and panel sizes last only for the session. Other check editors, HTTP Add to flow, YAML, compiler
  diagnostics, export/save, and native execution remain deferred. Request edits preserve richer
  Flow DSL document fields without converting expressions to literal URLs.

## [0.1.0-rc.4] - 2026-10-13

### Added

- Home with separate HTTP and Flows destinations, matching the HTML design baseline. HTTP
  drafts, responses, and running requests survive navigation between screens.
- Named request saving in local SQLite storage, reopening from Home and search after restart,
  and Save / Discard / Cancel protection when closing modified requests or quitting.
- Shared, persistent environment management with URL rebasing that preserves request paths,
  query parameters, and unrelated URLs.
- Native cURL import with live preview and request-code export/copy. Runnable examples use
  HTTPBingo consistently.
- History and global search dialogs, single-entry history deletion, and confirmation before
  clearing all history.

### Changed

- Rebuilt the HTTP request and response surfaces with GPUI Kit 0.7.3 and GPUI 0.3.9, including
  retained inputs, dialogs, menus, scrollbars, and persistent light/dark appearance.
- Added responsive request/response splitting, pointer and keyboard resizing, a panel-size
  dialog, and saved layout preferences.
- Aligned Params, Headers, Body, and Auth with the prototype. Body supports None, JSON, Raw,
  URL encoded, Form-data, and Binary while retaining each type's draft.
- Replaced custom Body, Scripts, and Tests editors with Kit Textarea, and the response body
  with a read-only Kit Editor. Pretty mode provides JSON highlighting and line numbers; Raw
  mode wraps text. Selection and scrolling are retained per request.
- Removed superseded custom input, text-layout, selection, and scrollbar implementations.

### Fixed

- Anchored response scrollbars to the visible viewport when resizing and switching layouts.
- Corrected request-table scrolling in constrained editor heights.
- Pinned the Vaultwarden E2E service image to the tested API baseline.

### Known limitations

- The native Flows destination does not yet provide a visual editor or execution UI. Flow DSL
  checking and execution remain available through the CLI.
- Saving is explicit. Unsaved drafts are not recovered after a crash or force quit. Saved
  requests retain explicitly entered credentials in unencrypted local storage; History
  redaction is independent.
- cURL import supports the documented inline subset. File-based and multipart commands are
  export-only; see [cURL support](docs/curl.md).
- Response bodies remain buffered text; byte-native viewing, atomic save-as, and incremental
  streaming progress are not implemented.
- Unsigned candidate packages can trigger macOS Gatekeeper or Windows SmartScreen warnings.

## [0.1.0] - Unreleased

This section summarizes the 0.1.0 prerelease series. Candidate builds are available on
[GitHub Releases](https://github.com/847850277/postman-gpui/releases); the final release is still pending.

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

### Changed

- `BodyInput::form_data_entry_count` now reads the form editor's rows directly instead of
  maintaining a duplicate count. This changes the public Rust signature from
  `form_data_entry_count(&self)` to `form_data_entry_count(&self, cx: &gpui::App)`.
  Rust callers must migrate `input.form_data_entry_count()` to
  `input.form_data_entry_count(cx)` using their current GPUI application context; this is
  a source-breaking change for consumers of the UI library. Both workspace callers have
  been migrated. Disabled and blank editor rows still count, and application behavior
  and persisted data formats are unchanged. Refs #190, #180.

### Known limitations

- Binary and range responses do not yet have a byte-native viewer.
- Large responses cannot yet be saved atomically to disk.
- Streaming responses do not yet expose incremental progress or stream cancellation semantics.
- Unsigned release candidates can trigger macOS Gatekeeper or Windows SmartScreen warnings. Final
  release packages should be signed using the credentials documented in `docs/releasing.md`.

[Unreleased]: https://github.com/847850277/postman-gpui/compare/v0.1.0-rc.4...HEAD
[0.1.0-rc.4]: https://github.com/847850277/postman-gpui/compare/v0.1.0-rc.3...v0.1.0-rc.4
[0.1.0]: https://github.com/847850277/postman-gpui/releases
