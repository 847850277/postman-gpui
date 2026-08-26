# Keyboard, Selection, and Clipboard Contract

This document is the stable interaction contract for application controls. The canonical GPUI
bindings live in `src/app/keyboard.rs`; feature-specific text actions remain scoped by their key
contexts so application commands do not steal ordinary editing keys.

## Application shortcuts

| Context | Key | Command | Platforms | Automated coverage |
| --- | --- | --- | --- | --- |
| Anywhere | `Cmd/Ctrl+Enter` | Send or cancel the active request | macOS / Windows / Linux | `tests/ui_keyboard.rs`, `tests/httpbingo_scenarios.rs` |
| Anywhere | `Cmd/Ctrl+T` | Create and activate a request tab | macOS / Windows / Linux | `tests/ui_keyboard.rs`, `tests/httpbingo_scenarios.rs` |
| Anywhere | `Cmd/Ctrl+W` | Close the active request tab | macOS / Windows / Linux | `tests/ui_keyboard.rs`, `tests/httpbingo_scenarios.rs` |
| Anywhere | `Cmd/Ctrl+L` | Focus the active request URL | macOS / Windows / Linux | `tests/ui_keyboard.rs`, `tests/httpbingo_scenarios.rs` |
| Anywhere | `Cmd/Ctrl+Shift+F` | Focus History search | macOS / Windows / Linux | `tests/ui_keyboard.rs`, `tests/httpbingo_scenarios.rs` |
| Anywhere | `Ctrl+Tab` | Activate the next request tab | macOS / Windows / Linux | `tests/ui_keyboard.rs`, `tests/httpbingo_scenarios.rs` |
| Anywhere | `Ctrl+Shift+Tab` | Activate the previous request tab | macOS / Windows / Linux | `tests/ui_keyboard.rs` |
| Anywhere | `Cmd/Ctrl+/` | Open or close shortcut help | macOS / Windows / Linux | `tests/ui_keyboard.rs` |
| Menu or overlay | `Escape` | Dismiss without changing request data | macOS / Windows / Linux | `tests/ui_keyboard.rs`, `tests/ui_clipboard.rs` |

`Cmd` is used on macOS and `Ctrl` is used on Windows and Linux unless a row explicitly says
`Ctrl` on every platform. The shortcut dialog in the application exposes the application-level
subset at runtime.

## Text and clipboard behavior

| Context | Key | Command | Mutation policy | Automated coverage |
| --- | --- | --- | --- | --- |
| Editable text | `Cmd/Ctrl+A` | Select all | Selection only | `tests/ui_clipboard.rs`, `tests/ui_keyboard.rs` |
| Editable or Response text | `Cmd/Ctrl+C` | Copy selection | Never mutates | `tests/ui_clipboard.rs` |
| Editable text | `Cmd/Ctrl+X` | Cut selection | One undoable edit | `tests/ui_clipboard.rs` |
| Editable text | `Cmd/Ctrl+V` | Paste | One undoable edit | `tests/ui_clipboard.rs` |
| Editable text | `Cmd/Ctrl+Z` | Undo | Local to the current projected request | `tests/ui_keyboard.rs`, `tests/ui_clipboard.rs` |
| Editable text | `Cmd+Shift+Z`, `Ctrl+Shift+Z`, or `Ctrl+Y` | Redo | Local to the current projected request | `tests/ui_keyboard.rs`, `tests/ui_clipboard.rs` |
| Text | Arrow / `Shift+Arrow` | Move / extend selection by grapheme | Unicode-safe | component tests and `tests/ui_clipboard.rs` |
| Text | `Alt+Arrow` or `Ctrl+Arrow` | Move by word | Unicode-safe | component tests |
| Text | `Home` / `End` | Move to boundary | Selection only | component tests |

Consecutive platform text commits form one undo transaction. Cursor movement, clipboard commands,
structural edits, request-tab projection, and History replay end that transaction. A projection
clears the local undo stack, so Undo can never restore content from another request tab or an older
History selection.

Password-like inputs render grapheme-count-preserving mask glyphs. They allow Paste, Select All,
Undo, and Redo, but Copy and Cut are deliberately unavailable from both shortcuts and context
menus. Response content is read-only and exposes Copy and Select All, never Cut or Paste.

## Focus and activation

`Tab` and `Shift+Tab` traverse visible tab stops in rendered order:

1. Header controls, then the New Request rail command.
2. History actions, search, and visible History rows.
3. Request tabs, New Tab, Method, URL, and Send/Cancel.
4. Request-pane tabs and the active pane's controls in visual row order.
5. Response-pane tabs, Copy, selectable response content, and any visible overlay controls.

Hidden pane controls are not tab stops. Dynamic rows keep focus on the nearest surviving toggle
after deletion; new and closed request tabs focus the active request tab. Shortcut Help restores the
control that opened it. All focused custom controls draw a border or background focus treatment.

`Enter` and `Space` activate buttons, tabs, toggles, list items, and radio options through the same
component command used by pointer activation. Arrow keys move through Method options, request and
response tab groups, authorization and body radio groups, and History rows.

Two redundant pointer affordances intentionally do not add extra tab stops:

- A request tab's small `×` closes the same active-tab command exposed as `Cmd/Ctrl+W`. Keeping it
  out of the tab sequence makes adjacent request tabs one step apart.
- Pointer-opened edit-menu items mirror the documented editing shortcuts. The originating input
  retains focus, the same commands remain available from the keyboard, and `Escape` closes the
  menu without mutation.

Static rail placeholders, labels, counters, status badges, and decorative cards are not
interactive and therefore are not included in focus order.
