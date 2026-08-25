# E2E Design Specification

This document is the normative design contract for UI-driven E2E feature
slices in Postman GPUI. The words **must**, **should**, and **may** describe
required, recommended, and optional behavior.

## 1. Ownership and Naming

- Each UI E2E issue must own exactly one issue design artifact.
- Issue artifacts must use this filename:

  ```text
  issue-<four-digit-issue-number>-<feature-slug>.pen
  ```

- The issue number in the filename, root-frame name, GitHub link, README
  mapping, and E2E contract must agree.
- A feature change updates its existing issue artifact. Do not create
  `final`, `v2`, `new`, or date-suffixed variants.
- Reusable starting points belong under `design/templates/` and are not issue
  artifacts.

## 2. Canonical Pencil Document

Every issue artifact must:

- use Pencil document version `2.17`;
- contain exactly one top-level root frame;
- use a valid UUID v4 `fileToken`;
- contain unique, non-empty node IDs;
- place its root at `x = 0`, `y = 0`;
- use a root width of `1600`;
- use `$bg`, vertical layout, `32px` padding, and `24px` section gaps;
- be tall enough to contain every section without clipping.

The standard root structure is:

```text
Issue #NN · Feature
├── Design Header
├── Application State · Feature       OR State Row …
├── Feature Flow / Pipeline            optional
└── E2E Contract
```

The first child must be `Design Header`. The last child must be
`E2E Contract`.

## 3. Canvas and Application Shell

### Design Header

- Width: fill container (`1536px` after root padding)
- Height: `150px`
- Must show the issue number, issue URL, feature title, current delivery
  status, endpoint or scope, and a one-sentence contract summary.

### Full application state

Use a full application state when the feature is primarily one editor and one
request/response lifecycle.

- Name: `Application State · <feature>`
- Width: fill container
- Height: `900px`
- App header: `72px`
- App body: remaining height
- Left rail: `64px`
- History panel: `250px`
- Request workspace: fill remaining width
- The workspace must render the request tab, request builder, feature editor,
  response panel, and relevant History result.

Headers, authorization, body editors, response behavior, and similar slices
must use this shell so differences remain feature differences rather than
layout differences.

### Multi-state matrix

Use a state matrix when comparison between states is the feature, for example
global search, empty/loading/error states, or keyboard selection.

- Keep the same `1600px` root, Design Header, palette, and E2E Contract.
- State cards must have numbered headings and a short behavioral note.
- Two-column rows should use two `756px` cards separated by a `24px` gap.
- Every state must show enough application context to identify where it
  occurs. A detached dialog without its owning screen is not sufficient.

### Optional flow or pipeline

- Height: `120–160px`
- Use only when transformation or lifecycle order is material.
- Show real product states, not architecture invented only for presentation.
- Typical sequence:

  ```text
  Rendered control → ViewModel → Request → Server echo → Response/History
  ```

## 4. Content Items

A feature editor must not be represented only by explanatory cards. Each
implemented input or result must appear as a concrete content item.

An input content item must show, when applicable:

1. field label;
2. current user-visible value;
3. enabled, selected, disabled, dirty, or validation state;
4. whether the value is already in the ViewModel;
5. its final outgoing request representation.

A response content item must show, when applicable:

1. HTTP status;
2. stable response subset used by the scenario assertion;
3. positive evidence for included values;
4. negative evidence for values that must be absent;
5. consistency between `ResponseState`, rendered response, and History;
6. a compact **Copy** action when a text response body is populated. The
   action copies the complete body from `ResponseState`, never a truncated
   visual subset, and is absent from empty or **Not sent** states. Byte-only
   responses expose **Save as…** instead and explicitly omit text Copy.

Sensitive values may be illustrative scenario values. Never place real
credentials or production secrets in a design artifact.

### Persistent History boundary

When a slice touches durable History, the design must distinguish the runtime
History projection from the versioned snapshot stored by the History
application service and repository.

- `HistorySnapshotV1` is request-only. `HistorySnapshotV2` may additionally
  persist a sanitized response snapshot for a completed HTTP exchange.
- Persist the sanitized replay request and response metadata. V2 may persist a
  textual response body up to `256 KiB`; larger bodies must be truncated with
  an explicit flag. Never persist binary, download, or streaming bodies;
  download spools or destinations; tabs, active-tab state, panes, drafts,
  dirty state, cookie jars; plaintext authorization/cookie values; or
  sensitive response headers such as `Set-Cookie`.
- Current-session replay may retain the complete Request in an ephemeral
  overlay keyed only by IDs already confirmed by SQLite. The overlay is not a
  second History source: it owns no rows, ordering, metadata, or render path;
  it is never persisted and is pruned whenever the authoritative query drops
  an ID. After restart, replay uses only the sanitized snapshot.
- Selecting a V2 snapshot with a stored response must restore the sanitized
  request into the active ViewModel and commit `ResponseState::Historical`
  without sending a network request. Status, sanitized headers, text body,
  Copy availability, and History metadata must remain consistent.
- Selecting V1 or any snapshot without a stored response must show the
  explicit unavailable state “This older History entry did not store a
  response.” It must not imply that a new request was sent.
- A historical response is read-only evidence. Editing the restored request
  must not mutate it. Clicking Send starts a new lifecycle
  `Historical → Loading → terminal`, appends a new History row on completion,
  and leaves the original entry unchanged.
- A versioned replay snapshot must include request options that affect wire
  behavior, including timeout and redirect policy / hop limits.
- Completed HTTP exchanges, including non-2xx responses, may persist exactly
  once. Cancelled and no-response failures must not persist, even if partial
  headers or bytes were observed.
- Storage read, append, migration, corruption, and clear failures must remain
  nonfatal and visibly distinguish durable rows from volatile-only rows.
- Persistence/restart tests should use an isolated SQLite database and a
  deterministic local server. Public echo examples still use the HTTPS
  HTTPBingo origin.

All public echo examples and E2E endpoint labels must use the HTTPS HTTPBingo
origin, `https://httpbingo.org`. Do not use the legacy HTTPBin origin or generic
placeholder origins in issue artifacts and templates.

## 5. Interaction States

- If Send can be clicked while an input remains active, show that active state
  and state that Enter, Tab, blur, or Add is not required.
- If a populated response exposes Copy, show its keyboard-reachable default
  state and document the brief **Copied** feedback without changing the active
  tab, scroll position, request, response, or History.
- If the ViewModel transforms a value on Send, distinguish the before-Send
  value, stored value, and outgoing representation.
- Disabled rows must remain visible if persistence is part of the contract and
  must be visibly excluded from the outgoing request.
- Keyboard-driven features must show focus or selection and the relevant key
  behavior.
- Empty and error states must include a recovery action when the product has
  one.

## 6. E2E Contract Section

The final `E2E Contract` section must be `480–500px` high and contain:

- GitHub issue number and parent roadmap issue;
- scenario and/or test file paths;
- ordered real-UI steps;
- observable contract assertions;
- included scope and explicit non-goals;
- the owning `.pen` path;
- the one-issue/one-design rule.

Tests must drive rendered controls. A design must not describe a ViewModel-only
shortcut as the product interaction.

## 7. Shared Tokens

Issue artifacts and templates must define exactly these shared values:

| Token | Value |
| --- | --- |
| `bg` | `#F4F7F3` |
| `panel` | `#FFFEFB` |
| `panelAlt` | `#EEF5F1` |
| `line` | `#CFE0D7` |
| `text` | `#20342B` |
| `muted` | `#526B60` |
| `subtle` | `#64786E` |
| `accent` | `#C64B2B` |
| `accentSoft` | `#FFF0E8` |
| `green` | `#0E7A4E` |
| `greenSoft` | `#E4F6EA` |
| `blue` | `#0F718B` |
| `blueSoft` | `#E6F4F7` |
| `code` | `#F0F5F1` |
| `codeText` | `#243D34` |
| `accentVivid` | `#F56B3D` |
| `accentInk` | `#3C1F16` |
| `fontBody` | `Inter` |
| `fontMono` | `JetBrains Mono` |

Do not introduce a one-off color when an existing semantic token expresses the
same role. Transparent shadow colors are allowed as effects.

## 8. Mapping and Traceability

- Add every issue artifact to `design/README.md`.
- Add a `Design Artifact` link to the owning GitHub issue.
- Include the scenario/test path inside the design contract.
- Keep completed slices open or closed according to issue tracking policy;
  design status must not be inferred from the GitHub issue state alone.

## 9. Template and Validation

Start new designs from:

```text
design/templates/e2e-feature.pen
```

Replace every `NN`, placeholder title, scenario path, and contract item. Add
feature-specific content items before treating the artifact as complete.

Run the automated contract locally with:

```bash
cargo test --test design_artifacts
```

The normal CI test job runs this integration test through
`cargo test --all-targets --all-features`.

## 10. Review Checklist

- [ ] Filename and issue number agree
- [ ] Exactly one `1600px` root frame
- [ ] Design Header is first and E2E Contract is last
- [ ] Full application shell or justified multi-state matrix is present
- [ ] Inputs and outputs are concrete content items
- [ ] Request and stable response evidence are visible
- [ ] Public endpoint examples use `https://httpbingo.org`
- [ ] ResponseState, View, and History lifecycle is represented
- [ ] `HistorySnapshotV1` request-only and V2 historical-response behavior are visibly distinct
- [ ] Historical responses are read-only; Send starts a new lifecycle and preserves the original entry
- [ ] Durable History is separated from tab, draft, cookie-jar, binary/stream, and secret state
- [ ] Populated live or Historical text responses expose Copy; byte-only responses expose Save as…; unavailable, empty, and Not sent states expose neither
- [ ] Shared tokens are unchanged
- [ ] README and GitHub issue links are present
- [ ] `cargo test --test design_artifacts` passes
