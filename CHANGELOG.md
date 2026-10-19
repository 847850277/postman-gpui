# Changelog

All notable changes to Postman GPUI are documented in this file. The project follows
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- Bound Flow file errors and short Console/empty-canvas scrolling, retain the full command bar,
  and keep file/execution actions reachable in compact windows. Explain Rust-source selections
  at Open Flow while preserving valid YAML filename variants and parser details.
- Strip credentials on same-port HTTPS-to-HTTP redirects, count redirect hops correctly, retain
  CLI overrides of nested file variables, and enforce loop deadlines around unlimited requests.

- Preserve response bytes before character decoding so binary and non-UTF-8 response sizes
  remain accurate in the response panel and History. Binary responses show a placeholder;
  Save as writes the original body bytes, including when the active response changes.
- Keep the full OPTIONS method visible in Flow request selectors and their dropdown menus.

### Added

- Release workflow candidates can be built from an explicit source ref without publishing a
  GitHub Release. All package jobs use the validated source commit; build metadata records the
  tag, commit, workflow attempt and verified signing status. Complete artifact sets and checksums
  are validated before upload/publication. Published release assets are never overwritten.
- Native Flow Run/Cancel from the toolbar and Console, with immutable document/input/environment
  snapshots, real HTTP execution, request/poll cancellation, and running-flow close protection.
- Detailed Steps/check/extraction and typed Outputs inspection, scoped nested loop iterations,
  partial failure evidence, revision-aware navigation, bounded retention and sensitive-value redaction.

- Native GPUI Kit Flow Save/Save as, Export copy, Open and durable recent-file reopening, with
  complete YAML document preservation and explicit external-file conflict handling. Hidden invalid
  drafts block writes; runtime overrides remain session-only and edits during saving remain unsaved.
- Flow saved/unsaved state and Save/Discard/Cancel protection coordinated with HTTP requests on
  normal window close/Quit. File, picker and index failures preserve existing documents and edits.

- Native Flow Check through the toolbar and Console, using the real compiler and all retained
  document draft errors. Per-Flow results become stale after edits; keyboard/pointer diagnostic
  links restore the owning editor and reject outdated locations.
- Retained GPUI Kit readonly YAML preview and Copy YAML, including compact windows. Native
  serialization preserves the complete document; pending edits and codec failures disable Copy,
  and runtime/environment values stay outside the preview.


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

- Native Flow library, sequential canvas, and HTTP step inspector using
  GPUI Kit. Create, rename, filter, append steps, and resume drafts from Home without losing edits.
- Responsive Flow library/inspector/results panels with pointer and keyboard resizing. Compact
  windows use a flow picker and Canvas/Details switching; the minimum window is now 768 × 600.
- Native Flow Console with Logs/Steps/Outputs, bounded per-flow command history, level filtering,
  copy/clear, auto-scroll, and collapse/maximize/restore. Local commands use retained GPUI Kit
  controls; `check`, `run` and `stop` use the native compiler and execution engine.
- Flow inspectors can expand beyond the former 480px cap while keeping the canvas usable.

### Limitations

- Unsaved Flow drafts, runtime overrides, execution results and Flow panel sizes remain
  session-only. Saved/exported documents reopen without losing readonly native DSL fields.
- Non-status check editors and API/loop configuration remain readonly; YAML is a readonly preview.
  General DAG editing, parallel scheduling and crash recovery are separate work.
- P7-1 establishes development-build acceptance. Candidate/platform installation, native OS input
  methods and final signing/release acceptance remain tracked in #249, #148 and #49.

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
