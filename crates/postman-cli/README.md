# postman-cli

`postman-cli` is the UI-free host for `.http` files and native `.http.yml` flows. The binary is
`postman-g`. Both file kinds compile to a `postman-flow` plan and run through the shared
`postman-http` / `postman-request` transport.

Run the checked-in HTTPBingo capability matrix:

```bash
cargo httpbingo-headless
```

Run one or more files/directories and override a binding:

```bash
cargo run --locked -p postman-cli -- run path/to/api.http --var host=https://example.com
cargo run --locked -p postman-cli -- run path/to/flow.http.yml --input client=hello
cargo run --locked -p postman-cli -- run tests/smoke.http tests/regression/
cargo run --locked -p postman-cli -- run path/to/flow.http.yml --check
```

`--var` and `--input` are aliases. Directories are searched recursively for `.http`, `.http.yml`,
`.http.yaml`, `.flow.yml`, and `.flow.yaml` files and run in sorted path order. Plain `.yml` files
are ignored. All files in one invocation share the same HTTP session, so cookie flows can span
files. A failure stops the remaining requests in that file while independent files still run and
appear in the suite report. Extra bindings that a file did not declare are ignored, so a mixed
directory can share one `--var host=...`.

`--check` parses and compiles without creating a transport or sending requests. Add `--json` for a
versioned machine-readable suite report, `--timeout-ms N` for a default request deadline,
`--no-follow-redirects` to return the first redirect response, or `-v`/`--verbose` for Flow debug
logs on stderr. A passing file exits with code `0`, an assertion or transport failure exits with
code `1`, and invalid CLI/file/compile input exits with code `2`.

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

## Execution and Exit Codes

- `0`: All requests in all suites passed.
- `1`: One or more assertions failed or unexpected transport errors occurred.
- `2`: CLI argument error, file read failure, or YAML/HTTP syntax compilation error.

This exit code contract allows `postman-g` to integrate seamlessly into CI/CD pipelines (e.g. GitHub Actions) and automated test platforms.

