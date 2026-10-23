# Saved requests

In the HTTP editor, **Save** or **Cmd+S** (macOS) / **Ctrl+S** (Windows/Linux)
commits the current request to local storage. First save asks for a name; the
pencil button renames an existing document. Later saves update the same identity.
No valid URL or complete JSON body is required to save an unfinished draft.

**Home → Saved requests** shows a preview of saved documents. **Browse…** and global
search list saved documents separately from open tabs and execution history.
Closing a tab does not delete its saved document. After restarting, reopen it
from Home or search; reopening an already-open document focuses that tab. The
application starts on Home and does not automatically reopen all previous tabs.

## What is retained

Documents preserve method, raw URL, ordered parameter/header rows (including
notes, duplicates, disabled and unfinished rows), each body mode's retained draft,
raw-body format, automatic/manual managed-header choices, authorization fields,
timeout/redirect options, and pre-request/test scripts. No sending, script
execution, or file upload occurs during Save or Open.

Shared environment settings remain separate. Opening a saved request preserves
its saved URL, even when another environment is selected. A later explicit
environment change rebases matching open drafts under the existing rules and
marks affected documents unsaved until saved again.

File fields store references, not file contents. Nonempty relative references
are made absolute against the working directory when saved, without opening the
files. Empty references remain editable. Files must still exist and be readable
when a request is sent; replacing or deleting a referenced file affects a later
send. Cached file-size hints are not persisted.

## Local storage and sensitive values

Request documents use `requests.sqlite3` in the platform's local data directory
under `postman-gpui`, alongside `environments.json` and the independent history
database. On macOS this is normally `~/Library/Application Support/postman-gpui`.
The schema and each document are versioned. SQLite transactions and optimistic
revisions protect the previous version from failed or stale writes.

Explicit Save retains **all entered request values**, including passwords,
tokens, Authorization/Cookie headers, query values and body content. This is
local, unencrypted storage, disclosed in the save dialog. New database files are
owner-readable/writable on Unix. Saving does not copy the runtime cookie jar,
script-produced values, responses, or transient transport state into a document.
History independently records completed sends, including their explicit request credentials,
without requiring Save. It restores the sent request; a saved document also retains disabled and
unfinished editor rows and later unsent edits. Historical response previews remain sanitized.
Do not put either database in a public/shared directory if it contains secrets.

If a database is unavailable or incompatible, restore access or a compatible
backup and use **Home → Saved requests → Reload**. Malformed individual records
are skipped with a visible warning and left unchanged; valid records remain
accessible. A conflicting external revision is not overwritten: preserve the
current draft and resolve the saved version before retrying.

## Unsaved changes and closing

A tab's dot means its editable content differs from its last committed document.
Sending a request never saves it. cURL imports and history replays contain unsaved
work; replay opens another tab when necessary to preserve an existing draft.
Edits made during a save remain unsaved after that save finishes.

Closing a modified tab, closing the application window, or using the app's Quit
command offers **Save / Discard changes / Cancel**. Save waits for the commit;
a failure keeps the draft and dialog available for retry. Discard keeps the last
saved version on disk. Cancel leaves tabs open. Quit processes modified requests
in order, and cancelling a later decision keeps the remaining session open.
An untouched empty tab closes without a prompt.

This is explicit saving, not autosave or crash recovery. Force termination,
crashes, and OS shutdown paths that bypass normal window/app commands cannot
recover unsaved drafts. The pinned GPUI runtime's final `on_app_quit` callback
cannot veto shutdown; confirmation happens before normal Quit/close commands.

## Design adaptations and verification

The reference is the HTML prototype at
[`00fd1df`](https://github.com/847850277/postman-gpui/blob/00fd1df9ced8eb36a273a03733c4ee863d2a35fc/prototypes/request-workspace.html).
The HTTP Save affordance, tab dot and status feedback follow that design. Its
browser Save only acknowledges the preview session. Native additions are a
compact Kit naming/close dialog, a rename action, and a Saved requests section
using the existing Home and Kit search-list surfaces. No Collections or workspace
hierarchy is added. Light remains the default; dialogs and toolbar actions adapt
to compact windows and dark mode.

Automated validation:

```sh
cargo test --locked --lib
cargo test --locked --test ui_saved_requests
cargo test --locked --test ui_workspace --test ui_keyboard --test ui_shell
```

Coverage includes all body/authorization/editor-intent round trips, restart and
reopen, duplicates, stale writes, malformed/unsupported data, write rollback,
input-focused keyboard Save, naming validation, modal command ownership,
Save/Discard/Cancel, multi-document Quit cancellation, and light/dark geometry
at 1440×960 and 960×640. Use HTTPBingo for runnable examples; network verification
uses deterministic local fixtures.
