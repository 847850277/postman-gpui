# postman-cli

`postman-cli` is the UI-free `.http` host for the shared `postman-http` contract and the
`postman-request` transport.

Run the checked-in HTTPBingo capability matrix:

```bash
cargo httpbingo-headless
```

Run one or more files/directories and override a file variable:

```bash
cargo run --locked -p postman-cli -- run path/to/api.http --var host=https://example.com
cargo run --locked -p postman-cli -- run tests/smoke.http tests/regression/
```

Directories are searched recursively for `.http` files and run in sorted path order. All files in
one invocation share the same HTTP session, so cookie flows can span files. A failure stops the
remaining requests in that file while independent files still run and appear in the suite report.

Add `--json` for a versioned machine-readable suite report, `--timeout-ms N` for a default request deadline, or
`--no-follow-redirects` to return the first redirect response. A passing file exits with code `0`,
an assertion or transport failure exits with code `1`, and invalid CLI/file input exits with code
`2`.

## Supported `.http` subset

- `###` request separators
- `@name = value` file variables and `{{name}}` interpolation
- GET, POST, PUT, PATCH, DELETE, HEAD, and OPTIONS request lines
- headers and JSON, raw, or URL-encoded text bodies
- `#` and `//` comments
- per-request `@timeout-ms N`, `@redirect follow|no-follow`, and `@max-redirects N` wire options
- sequential response assertions and captures:

```http
@host = https://httpbingo.org

### Generate an id
# @name seed
# @assert status == 200
# @capture request_id = jsonpath "$.uuid"
GET {{host}}/uuid

### Reuse it
# @name echo
# @assert status == 200
# @assert jsonpath "$.json.request_id" == "{{request_id}}"
POST {{host}}/anything/{{request_id}}
Content-Type: application/json

{"request_id":"{{request_id}}"}
```

Supported assertion forms are `status == CODE`, `redirects == COUNT`,
`error == timeout|redirect-limit|network|invalid-request|invalid-response|response-too-large|cancelled`,
`header "Name" exists`, `header "Name" contains VALUE`, `body contains VALUE`, and
`jsonpath "$.path[0]" == VALUE`. An expected transport error is a passing flow step, so timeout and
redirect-limit behavior can be tested without making the whole suite fail.

## HTTPBingo coverage

`cargo httpbingo-headless` executes 6 focused files and 67 live requests. The checked-in
[`coverage.json`](tests/fixtures/httpbingo/coverage.json) inventories all 58 endpoint families from
a pinned go-httpbin revision: 45 are fully covered, 6 are exercised with documented limitations,
6 need a new client/model capability, and `/brotli` is intentionally unavailable upstream. A
deterministic test verifies that the inventory remains complete and that every covered entry points
to executable `.http` evidence.

The remaining model gaps are lossless binary responses, Digest authentication, incremental stream
events/chunks, HTTP trailers, multipart external-file syntax, and WebSocket frames. Arbitrary
JavaScript, branching, and parallel requests also remain outside this slice; unsupported syntax
returns a source-located diagnostic.
