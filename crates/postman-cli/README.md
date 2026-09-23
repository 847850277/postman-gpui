# postman-cli

`postman-cli` is the UI-free host for `.http` files and native `.http.yml` flows. The binary is
`postman-g`. Both file kinds compile to a `postman-flow` plan and run through the shared
`postman-http` / `postman-request` transport.

## Installation

### One-line installer (macOS & Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/847850277/postman-gpui/main/install.sh | bash
```

### Install with Cargo

```bash
# Install from GitHub repository
cargo install --git https://github.com/847850277/postman-gpui postman-cli --bin postman-g

# Or install from local workspace
cargo install --path crates/postman-cli --bin postman-g
```

### Prebuilt binaries

Download standalone `postman-g` binaries from [GitHub Releases](https://github.com/847850277/postman-gpui/releases).

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

## Flow return values

Each file's report includes an outputs object and a redacted_outputs array. These are additive
fields in the version 1 suite report. Native YAML returns the values declared in flow.outputs;
the .http adapter declares its captures as flow outputs, so those values are now reported too.
Objects, arrays, numbers, booleans, null and strings retain their JSON types.

~~~json
{
  "success": true,
  "requests": [],
  "outputs": {
    "order_data": {"count": 1, "list": [{"id": "00123", "paid": true}]},
    "token": "[REDACTED]"
  },
  "redacted_outputs": ["token"]
}
~~~

In --json mode, read these fields at files[i].report.outputs and
files[i].report.redacted_outputs. Human-readable reports print an OUTPUT line for each return.
Sensitive values, as marked by a Flow input or export's sensitive: true, are replaced with
the string "[REDACTED]" before the RunReport is constructed. The redacted_outputs list
distinguishes these from an ordinary literal "[REDACTED]" value. An output object marked
sensitive is redacted as a whole; unmarked output values are returned unchanged.
Failed flows and flows without declared returns have empty output maps.

The CRMEB YAML/Rust example now exports $.data from order-list as order_data.
It does not declare its intermediate login key or token as flow returns. To run the YAML:

~~~sh
cargo run --locked -p postman-cli -- run \
  crates/postman-flow/examples/flows/crmeb_order_list.http.yml --json
~~~

## Loop progress and reports

Native YAML `for_each` and `repeat_until` steps report live progress on stderr in human mode:

```text
LOOP wait-task [1/3] — running
WAIT wait-task [1/3] — interval 1000 ms
LOOP wait-task [2/3] — running
PASS LOOP wait-task [2/3] — condition_met (1035 ms)
```

The final stdout report includes each loop's iteration outcomes, body times, actual waiting
times, configured wait intervals, total elapsed time, and termination reason. Requests inside
nested loops carry a path such as `outer[2/3] > wait-task[1/5] > query`. Loop invocations and HTTP
requests have separate counts; loops skipped by `when` are counted as skipped loops.

With `--json`, live progress is disabled and stdout remains one JSON suite report. The existing
version 1 fields remain; files with loops additionally include `files[i].report.loops`:

```json
{
  "step_id": "wait-task",
  "name": "wait-task",
  "kind": "repeat_until",
  "limit": 3,
  "loop_path": [],
  "total_executed": 2,
  "elapsed_ms": 1035,
  "success": true,
  "skipped": false,
  "reason": "condition_met",
  "iterations": [
    { "iteration": 1, "success": true, "elapsed_ms": 17, "error": null,
      "wait_interval_ms": 1000, "wait_elapsed_ms": 1001 },
    { "iteration": 2, "success": true, "elapsed_ms": 17, "error": null }
  ],
  "captures": [],
  "error": null
}
```

Iteration numbers are **one-based** in both CLI text and JSON. Nested loop reports include their
enclosing iterations in `loop_path`; each invocation has its own report, even when its step ID
repeats. Requests inside loops have the same `loop_path` field, including their immediate loop.
Flat request reports omit it, and files without loops omit `loops`.

Termination reasons are `completed`, `condition_met`, `failure_condition`, `max_iterations`,
`timeout`, `step_failed`, and `cancelled`. A skipped loop has `skipped: true`, `reason: null`, and
no iterations. The iteration's `success` describes its body: successful HTTP requests can still
lead to a failed loop because its condition failed, its limit was exhausted, or its deadline
expired. Use the loop reason and file-level `success` to assess the whole result. A failure
handled by `on_error: continue` remains visible in the report even if the enclosing flow passes.

Loop timing is wall-clock time observed by the CLI; body timing excludes the following polling
wait, and the actual wait may be shorter than the configured interval at the loop deadline.
Progress and reports include export names, never raw intermediate export values.
Rust callers can use `run_flow_with_progress` to receive `LoopProgress` notifications while
retaining the same final `RunReport` returned by `run_flow`.

## Execution and Exit Codes

- `0`: All flows in the suite completed successfully under their configured error policies.
- `1`: A flow failed, including an unhandled assertion/transport failure or loop failure/timeout.
- `2`: CLI argument error, file read failure, or YAML/HTTP syntax compilation error.

This exit code contract allows `postman-g` to integrate seamlessly into CI/CD pipelines (e.g. GitHub Actions) and automated test platforms.
