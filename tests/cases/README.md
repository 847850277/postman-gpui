# Request Scenario Test Specification

The JSON documents below `tests/cases` are the executable product contract for
the request workflow. Each document owns one functional area, and each scenario
describes a request draft, the server response, and the observable result
expected from the application. A file explicitly targets either deterministic
local execution or the public HTTPBingo compatibility service.

## Scope

The local runner in `tests/request_scenarios.rs` verifies the complete path below:

```text
Scenario draft
  -> WorkspaceViewModel
  -> exact Request
  -> RequestExecutor
  -> local mockito server
  -> ResponseState and shared history
```

The deterministic suite covers request construction, HTTP delivery,
response-state transitions, and history behavior without depending on the
internet.

The opt-in runner in `tests/httpbingo_scenarios.rs` is an application-level
scenario runner:

```text
Scenario draft
  -> fresh TestAppContext window with a real PostmanApp View
  -> method/URL/params/headers/auth/body controls
  -> click Send
  -> WorkspaceViewModel and real RequestExecutor
  -> https://httpbingo.org
  -> ResponseState, rendered response content, system clipboard, and shared history
```

It uses the rendered method dropdown, URL field, query/header row editors,
authorization field, body-kind controls, and Send button. Every user action is
delivered through the rendered controls; the injected `WorkspaceViewModel` is
only observed for assertions, so the runner has no second `PostmanApp` command
surface. The runner then compares the complete request recorded by the real
application's history, checks the stable HTTPBingo echo, and verifies that the
response content is present in the window. Every populated response also clicks
the quick-copy action, compares the system clipboard with the exact
`ResponseState` body, and verifies that the action did not mutate request,
response, History, or active-tab state. Empty response bodies must not expose
the action.
It covers public DNS, TLS, redirects, request encoding, methods, headers, bodies,
and non-2xx responses, but does not replace the deterministic local contract.

## File organization

The contract is split by the primary product rule being specified:

```text
tests/cases/
├── request/
│   ├── methods.json
│   ├── query_params.json
│   ├── headers.json
│   ├── authorization.json
│   ├── json_body.json
│   ├── form_body.json
│   ├── raw_body.json
│   ├── file_upload.json
│   └── validation.json
├── response/
│   ├── status.json
│   └── content.json
├── history/
│   └── request_history.json
├── environment/
│   └── startup_checks.json
└── httpbingo/
    ├── methods.json
    ├── query_params.json
    ├── headers.json
    ├── authorization.json
    ├── bodies.json
    └── responses.json
```

The runner recursively discovers every JSON file in this tree. There is no
monolithic scenario file or compatibility path. A reserved functional-area file
may temporarily contain an empty `cases` array, but the tree as a whole must
define at least one scenario.

Place a cross-cutting scenario in the file for its main rule. For example, a
POST request asserting automatic JSON headers belongs in `json_body.json`, while
a POST request asserting that a manually supplied content type wins belongs in
`headers.json`.

## Running the suite

Run only the request scenarios:

```sh
cargo scenarios
```

The alias expands to:

```sh
cargo test --test request_scenarios
```

Run the real HTTPBingo compatibility scenarios explicitly:

```sh
cargo httpbingo-scenarios
```

The HTTPBingo test is ignored during ordinary `cargo test` runs because it
requires a public service and currently takes about 20–30 seconds. The explicit
command treats network or HTTPBingo failures as real test failures.

CI runs the same command in the standalone `HTTPBingo E2E` workflow. It runs for
every pull request, can be started manually with `workflow_dispatch`, and runs
automatically on weekdays at 03:17 UTC. A new commit to the same pull request
cancels its stale E2E run and starts the updated one. Because this check depends
on a public service, an HTTPBingo outage can make the pull-request check fail.

Locally, `cargo test` still runs the deterministic suite with the rest of the tests.
CI runs it as a separate **Request Scenario Contract** job so a contract
failure is visible on its own and does not share a log with the GPUI UI tests.

## Isolation model

Every local case starts with a new `WorkspaceViewModel`. A `target: "local"`
case containing `mock` receives a fresh mockito server and an automatically
assigned origin. Every `target: "httpbingo"` case starts with a fresh
`PostmanApp` and GPUI window, must omit `mock`, and receives the
`https://httpbingo.org` origin from the opt-in runner. Therefore, `draft.path`
and `expect.request.path` are host-relative in both modes, for example
`/users/42?active=true`.

Issue #59 also links its independently runnable GET `/forms/post` and POST
`/post` cases in one additional application-level workflow. That workflow
creates a second request through the rendered New Request action, keeps both
tab responses and shared History entries, and submits the final active form
cell without an intermediate commit action. No other scenario shares state.

Cases that fail before network delivery, such as an empty URL, omit `mock`. They
must expect an error response and no history entry.

## Scenario file structure

Every JSON document contains a schema version and a list of cases:

```json
{
  "schema_version": 5,
  "target": "local",
  "cases": []
}
```

`target` is required and accepts `local` or `httpbingo`.

Each case has four responsibilities:

- `name`: a unique description of one product rule;
- `draft`: user-editable request state applied to the ViewModel;
- `mock`: the local HTTP response and transport expectation for local cases;
- `expect`: the exact outgoing request, resulting response state, and history
  length.

### Draft fields

- `method`: one of the methods supported by `HttpMethod`;
- `path`: host-relative path, optionally including a query string;
- `params`: query rows with `key`, `value`, and optional `enabled`;
- `headers`: request-header rows with `key`, `value`, and optional `enabled`;
- `body`: request body text or `null`;
- `body_kind`: `none`, `json`, `raw`, `url_encoded`, `multipart`, or `null`;
- `body_rows`: ordered text-form rows with `key`, `value`, and optional
  `enabled`; supported by URL-encoded and text-only multipart drafts;
- `multipart_parts`: ordered, explicitly typed multipart rows. A Text part uses
  `{"kind":"text","name":"...","value":"..."}`; a File part uses
  `{"kind":"file","name":"...","path":"tests/fixtures/...","file_name":"...","content_type":"..."}`.
  Both variants accept optional `enabled`; disabled parts remain editor intent
  but are omitted from the effective request and from `expect.request`.
  File paths are repository-relative, must resolve to a file inside the
  repository, and reject `..`, absolute paths, and symlink escapes;
- `precreate_body_rows`: total rendered form rows to create before typing,
  including intentionally blank draft rows;
- `bearer_token`: token text, with or without the `Bearer` prefix;
- `basic_auth`: object containing `username` and `password` for HTTP Basic Auth.

`bearer_token` and `basic_auth` are mutually exclusive. The runner rejects a
scenario that supplies both instead of choosing one implicitly.

`enabled` defaults to `true`.

### Mock fields

- `status`: HTTP status returned by the local server;
- `headers`: response headers returned by the local server;
- `body`: response body returned by the local server.

The mock is matched against `expect.request`, so a scenario verifies both the
logical `Request` and what the HTTP client actually delivers over the wire.

### Expected result fields

`expect.request` is compared with the assembled `Request` exactly, including
method, URL path, header list, header ordering, and body. In HTTPBingo mode the
comparison uses the completed request recorded by the running application's
shared history, in addition to assertions against HTTPBingo's echoed response.
`expect.request.body_kind` is optional; use `multipart` when its URL-encoded
`body` notation represents ordered typed text parts rather than raw wire bytes.
Multipart requests containing files instead declare `multipart_parts` in both
`draft` and `expect.request`, leaving `body` as `null`; this keeps file paths and
metadata typed instead of encoding them as an `@path` text convention.
The multipart boundary remains transport-generated and is never stored in the
scenario's logical Request or History expectation.

A successful response supports:

- `status`: exact status code;
- `body_contains`: optional stable body fragment;
- `body_json_contains`: optional recursive JSON object subset. This is useful
  for HTTPBingo because dynamic fields such as origin IP and proxy headers are
  intentionally omitted from the expected subset;
- `headers_contain`: optional response-header subset. Header names are matched
  case-insensitively and values exactly.

An error response uses `contains` to match a stable fragment of the error
message. `history_len` specifies the expected shared-history size after sending.

## Authoring rules

1. Describe one observable product rule per case and keep case names unique.
2. Use host-relative paths. Local cases use local mocks; only files explicitly
   targeting `httpbingo` may call the public compatibility service.
3. Assert the complete outgoing `Request`, not only the mock response.
4. Prefer stable response fragments and meaningful headers over incidental
   formatting or timing values.
5. Include disabled rows, error paths, and state transitions when they are part
   of the rule.
6. Add a regression scenario before fixing a reproduced request-workflow bug.
7. Keep scenarios independent; no case may rely on another case's history or
   server state.
8. Put the case in the functional-area file that owns its primary product rule.
9. Keep HTTPBingo assertions limited to stable echoed values; never assert
   origin IPs, proxy headers, dates, timing, or header ordering.

## Contract evolution

Unknown JSON fields are rejected so spelling mistakes cannot silently weaken a
test. Scenario names must be unique across all files. When changing the scenario
format, update `SCENARIO_SCHEMA_VERSION`, every JSON `schema_version`, this
specification, and the runner in the same change.

Use the normal red-green-refactor cycle: add or update the scenario, observe the
expected failure, implement the smallest product change, and run the full test
suite before merging.
