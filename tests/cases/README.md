# Request Scenario Test Specification

`request_scenarios.json` is the executable product contract for the request
workflow. A scenario describes a request draft, the local server response, and
the observable result expected from the application.

## Scope

The Rust runner in `tests/request_scenarios.rs` verifies the complete path below:

```text
Scenario draft
  -> WorkspaceViewModel
  -> exact Request
  -> RequestExecutor
  -> local mockito server
  -> ResponseState and shared history
```

The suite covers request construction, HTTP delivery, response-state transitions,
and history behavior. GPUI layout and direct mouse or keyboard interaction remain
the responsibility of the `ui_*` integration tests.

The suite deliberately does not depend on public services, fixed ports, response
elapsed time, or exact pretty-printed JSON whitespace.

## Running the suite

Run only the request scenarios:

```sh
cargo scenarios
```

The alias expands to:

```sh
cargo test --test request_scenarios
```

The scenarios also run as part of the normal `cargo test` suite.

## Isolation model

Every case starts with a new `WorkspaceViewModel`. A case containing `mock`
also receives a fresh mockito server and an automatically assigned origin.
Therefore, `draft.path` and `expect.request.path` must be host-relative, for
example `/users/42?active=true`.

Cases that fail before network delivery, such as an empty URL, omit `mock`. They
must expect an error response and no history entry.

## Scenario structure

The document contains a schema version and a list of cases:

```json
{
  "schema_version": 2,
  "cases": []
}
```

Each case has four responsibilities:

- `name`: a unique description of one product rule;
- `draft`: user-editable request state applied to the ViewModel;
- `mock`: the local HTTP response and transport expectation, when applicable;
- `expect`: the exact outgoing request, resulting response state, and history
  length.

### Draft fields

- `method`: one of the methods supported by `HttpMethod`;
- `path`: host-relative path, optionally including a query string;
- `params`: query rows with `key`, `value`, and optional `enabled`;
- `headers`: request-header rows with `key`, `value`, and optional `enabled`;
- `body`: request body text or `null`;
- `body_kind`: `json`, `form_data`, `raw`, or `null`;
- `bearer_token`: token text, with or without the `Bearer` prefix.

`enabled` defaults to `true`.

### Mock fields

- `status`: HTTP status returned by the local server;
- `headers`: response headers returned by the local server;
- `body`: response body returned by the local server.

The mock is matched against `expect.request`, so a scenario verifies both the
logical `Request` and what the HTTP client actually delivers over the wire.

### Expected result fields

`expect.request` is compared with the assembled `Request` exactly, including
method, URL path, header list, header ordering, and body.

A successful response supports:

- `status`: exact status code;
- `body_contains`: optional stable body fragment;
- `headers_contain`: optional response-header subset. Header names are matched
  case-insensitively and values exactly.

An error response uses `contains` to match a stable fragment of the error
message. `history_len` specifies the expected shared-history size after sending.

## Authoring rules

1. Describe one observable product rule per case and keep case names unique.
2. Use host-relative paths and local mocks; never call a public test service.
3. Assert the complete outgoing `Request`, not only the mock response.
4. Prefer stable response fragments and meaningful headers over incidental
   formatting or timing values.
5. Include disabled rows, error paths, and state transitions when they are part
   of the rule.
6. Add a regression scenario before fixing a reproduced request-workflow bug.
7. Keep scenarios independent; no case may rely on another case's history or
   server state.

## Contract evolution

Unknown JSON fields are rejected so spelling mistakes cannot silently weaken a
test. When changing the scenario format, update `SCENARIO_SCHEMA_VERSION`, the
JSON `schema_version`, this specification, and the runner in the same change.

Use the normal red-green-refactor cycle: add or update the scenario, observe the
expected failure, implement the smallest product change, and run the full test
suite before merging.
