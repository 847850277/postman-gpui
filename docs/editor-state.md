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

## Implementation entry points

| Owner | Responsibility |
| --- | --- |
| [Request composer](../src/app/postman_app/request_workspace/composer.rs) and its [panes](../src/app/postman_app/request_workspace/panes) | Route input events to the active draft, restore controls from the model, and dispatch Send. |
| [TableCellInput](../src/ui/components/input/table_cell_input.rs) | Retain Kit `InputState`, emit changes with stable cell identity, and skip equal-value projections to preserve selection and undo. |
| [MultilineInput](../src/ui/components/input/multiline_input.rs) | Retain Kit `TextareaState` for JSON/Raw Body, Scripts, and Tests. Forward `InputEvent::Change`; explicit model projection resets undo and composition even when text is equal. |

Keep Kit entities and subscriptions alive in their owning view. Let Kit handle selection, IME,
clipboard, and editing history; keep request validation and draft mutations in application code.

## Regression checks

| Coverage | Tests |
| --- | --- |
| Active URL, Params, Headers, JSON/Raw, URL-encoded and multipart edits reach the outgoing request | [ui_send.rs](../tests/ui_send.rs) |
| Authorization, per-tab drafts, row restoration, Scripts and Tests | [ui_workspace.rs](../tests/ui_workspace.rs) |
| Cut/paste, deletion, focus traversal and keyboard actions | [ui_keyboard.rs](../tests/ui_keyboard.rs) |
| Silent projection, IME, selection and undo boundaries | Unit tests in [TableCellInput](../src/ui/components/input/table_cell_input.rs) and [MultilineInput](../src/ui/components/input/multiline_input/tests.rs) |

Run from the repository root:

```bash
cargo test --locked -p postman-gpui --lib ui::components::input
cargo test --locked -p postman-gpui --lib send_command_is_built_only_from_the_view_model
cargo test --locked -p postman-gpui --test ui_send --test ui_workspace --test ui_keyboard
```

The active-field scenarios send directly from the focused editor; transport assertions inspect
requests received by local HTTP fixtures. The composer unit test separately verifies that Send
uses the model. For release artifacts, also complete the installed-app
[smoke checklist](release-smoke-test.md#request-workflow); automated tests do not establish
package-level behavior on every operating system.
