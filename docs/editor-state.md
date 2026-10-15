# Editor state and synchronization

GPUI Kit owns input behavior; the application owns request drafts and synchronization.
This contract covers edits within a running session. Cross-restart persistence is documented
in [saved requests](saved-requests.md).

## State contract

- User edits update the active `RequestViewModel` in `WorkspaceViewModel` through change events.
  Typing, deletion, cut/paste, and undo/redo must not require Enter, Tab, blur, Add, or Send to commit.
- Send validates the current draft and builds the request from the model. It must not read focused
  controls to recover edits that were never synchronized.
- Switching tabs or request panes restores each draft, including blank and disabled rows and
  Body, Scripts, and Tests content. Row identity must survive insertion and removal.
- Projecting model values into controls is silent: it must not emit another domain edit or create
  a feedback loop. IME composition and undo history must not leak between request contexts.
- Response views share an immutable response snapshot. Draft edits must not copy the response
  body, reformat it, or reset its selection and scroll position. JSON parsing, Pretty formatting,
  and syntax scanning run on the background executor; the owning view installs the prepared
  document into its retained Kit `EditorState`. A newer response or Pretty/Raw change supersedes
  pending work, and closing a tab releases its editor. Full-response copy uses the original body.

Flow step configuration follows the same live-edit rule, with a separate typed/source boundary:
`FlowLibrary` owns `FlowDocument`; a retained `RequestInspector` per `(FlowId, step ID)` owns
raw URL/header/body/condition buffers, stable check/export row IDs, errors, tabs, disclosures, and per-pane scroll. Valid source edits replace only
the corresponding native field. Invalid edits advance the session revision but leave its last valid
value intact. Context switches reuse the existing entities; rendering never projects source text.
Representation switches are allowed only when lossless and preserve an equivalent alternate buffer.

Status checks retain their ordered native list, including readonly kinds. Blank status removes only
that typed check; invalid source retains it. Export rows keep separate last-valid projections; native
name/JSONPath validation and duplicate reconciliation prevent another edit from claiming an invalid
row's reserved name. Row removal chooses a surviving input (or Add action) for focus. Runtime values
are never read. Export changes refresh only derived hints in cached later inspectors; their text,
composition and undo entities stay untouched, and references are never rewritten. `when` shares the
native condition wire codec and its source/depth protections. Blank removes it; invalid nonempty
source advances revision without replacing its last-valid typed expression. Only top-level inline
HTTP definitions are editable; API/loop configuration stays readonly. Whole-flow compilation is not
part of keystroke processing.

## Implementation entry points

| Owner | Responsibility |
| --- | --- |
| [Request composer](../src/app/postman_app/request_workspace/composer.rs) and its [panes](../src/app/postman_app/request_workspace/panes) | Route input events to the active draft, restore controls from the model, and dispatch Send. |
| [TableCellInput](../src/ui/components/input/table_cell_input.rs) | Retain Kit `InputState`, emit changes with stable cell identity, and skip equal-value projections to preserve selection and undo. |
| [MultilineInput](../src/ui/components/input/multiline_input.rs) | Retain Kit `TextareaState` for JSON/Raw Body, Scripts, and Tests. Forward `InputEvent::Change`; explicit model projection resets undo and composition even when text is equal. |
| [Flow Request inspector](../src/app/postman_app/flows/request_inspector.rs) | Retain per-step Kit inputs and multiline sources; emit identity-scoped edits using the shared Flow field codec, without cloning or parsing the entire document. |
| [ResponseViewer](../src/app/postman_app/request_workspace/response_panel.rs) and [ResponseBody](../src/app/postman_app/request_workspace/response_panel/body.rs) | Share response snapshots, retain each tab's Kit `EditorState`, and reject superseded background formatting results. |

Keep Kit entities and subscriptions alive in their owning view. Let Kit handle selection, IME,
clipboard, and editing history; keep request validation and draft mutations in application code.

## Regression checks

| Coverage | Tests |
| --- | --- |
| Flow checks/exports/conditions, stable rows, cached hints, source isolation, and native codecs | [Inspector tests](../src/app/postman_app/flows/request_inspector/tests/definitions.rs) and [field codec tests](../crates/postman-flow/tests/field_expressions.rs) |
| Active URL, Params, Headers, JSON/Raw, URL-encoded and multipart edits reach the outgoing request | [ui_send.rs](../tests/ui_send.rs) |
| Authorization, per-tab drafts, row restoration, Scripts and Tests | [ui_workspace.rs](../tests/ui_workspace.rs) |
| Cut/paste, deletion, focus traversal and keyboard actions | [ui_keyboard.rs](../tests/ui_keyboard.rs) |
| Silent projection, IME, selection and undo boundaries | Unit tests in [TableCellInput](../src/ui/components/input/table_cell_input.rs) and [MultilineInput](../src/ui/components/input/multiline_input/tests.rs) |
| Large response reuse, background completion, Pretty/Raw races, tab lifecycle, readonly copy and scrolling | [Response panel tests](../src/app/postman_app/request_workspace/response_panel/tests.rs) and unit tests in [ResponseBody](../src/app/postman_app/request_workspace/response_panel/body.rs) |

Run from the repository root:

```bash
cargo test --locked -p postman-gpui --lib ui::components::input
cargo test --locked -p postman-gpui --lib send_command_is_built_only_from_the_view_model
cargo test --locked -p postman-gpui --lib response_panel
cargo test --locked -p postman-gpui --test ui_send --test ui_workspace --test ui_keyboard
```

The active-field scenarios send directly from the focused editor; transport assertions inspect
requests received by local HTTP fixtures. The composer unit test separately verifies that Send
uses the model. For release artifacts, also complete the installed-app
[smoke checklist](release-smoke-test.md#request-workflow); automated tests do not establish
package-level behavior on every operating system.

### Flow inputs and run parameters

A retained Inputs inspector per FlowId owns stable declaration-row IDs, name/default source,
Text/JSON modes, errors, sensitivity revelation, runtime overrides, focus and scroll state.
Only definition events update `FlowDefinition.inputs`; malformed definitions keep the last valid
row and mark the session dirty. Runtime edits and committed environment changes do not change
the document revision. Renames move an override with its row; deletion removes that binding and
new rows never reuse its identity. Existing expressions are not rewritten.

The explicit parameter-preparation boundary returns native `FlowInputs` or local errors. An
invalid enabled override or definition draft blocks preparation. Precedence is override, then
environment for declared `base_url`, then default, then missing. Null/false/zero/empty string are
present values; absence and invalid JSON are different states. JSON parsing reuses the bounded
Flow document field codec. Preparation does not compile or execute requests.

Sensitive editors are omitted from the rendered/accessibility surface until revealed. Leaving
Inputs, its view, a flow, or the Flow page conceals them while retaining source. Debug formatting
of `FlowInputs` lists names only. Defaults remain intentional document data; run overrides never
enter serialization, history, Console, or environment storage.

### Flow step structure

The Flow library owns adjacent reordering and removal by `(FlowId, step ID)`, separately from
canvas selection navigation. Successful mutations increment revision once; boundary moves,
missing targets, and cancelled dialogs do not. Reordering keeps every typed definition and
retained Kit editor intact, including invalid source, tab, scroll, composition and undo history.
All cached inspectors refresh available-variable names without modifying references.

Removal confirmation captures both IDs and revalidates the active flow before applying. Only
deleted inspectors and their owner subscriptions are released; delayed events cannot change a
replacement node. Nested loop definitions are removed as a unit. Owned layout entries are
removed while entries still used by surviving scoped IDs remain. Selection falls to the next
step, then previous, or Add step for an empty flow. Inputs overrides and Console state survive.
Session-only reserved IDs include imported/nested IDs and deleted IDs, preventing reuse by Add
step. These reservations are not DSL fields or restart persistence. Reference repair and
structural undo are outside this slice; compiler diagnostics remain P5.3.
