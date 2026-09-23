# Meilisearch flows

These seven flows exercise API keys, document ingestion/CRUD, search/ranking,
streaming NDJSON imports, multi-search, and facets against Meilisearch.

## Run

From the repository root, build a CLI that supports `repeat_until`:

```bash
cargo build --locked -p postman-cli --bin postman-g

# Fresh Docker instance; all seven flows in dependency order.
./crates/postman-flow-e2e/suites/meilisearch/run.sh flows

# One scenario. Search-only scenarios automatically run their ingestion fixtures.
./crates/postman-flow-e2e/suites/meilisearch/run.sh flows/hackernews_query.http.yml

# Check YAML without starting Docker.
./crates/postman-flow-e2e/suites/meilisearch/run.sh flows --check
```

The script accepts an existing file path or a path relative to this suite. `flows`,
the suite directory, and no target all select the full suite. It forwards CLI
options such as `--json`, `--input name=value`, and `--timeout-ms 30000`.
With `--json`, stdout contains one JSON report per executed flow (including
prerequisites); progress and infrastructure messages go to stderr.

`POSTMAN_G` selects the CLI binary. Otherwise the script looks in the workspace's
`target/debug`, `target/release`, and PATH, then installs from the internet if no
binary is found. YAML is checked before Docker starts; older releases without
loop support must be upgraded or built from this branch.

`PORT` defaults to `7700`. `MEILISEARCH_MASTER_KEY` and `MEILISEARCH_IMAGE` override
the test key and Docker image. The script creates its own container and removes
it on exit. A busy port fails without stopping an existing service.

The runner supplies the bundled `data/hackernews_sample.ndjson`. Use
`--input dataset_path=/absolute/path/to/data.ndjson` to import another dataset;
the separate query flow asserts the bundled sample's known content and counts.

## Task completion belongs to the flow

All **16 asynchronous operations** export `$.taskUid` from the HTTP 202 response
and immediately run a `repeat_until` step against `GET /tasks/{taskUid}`:

- `succeeded` allows the next operation to run.
- `failed` or `canceled` terminates the flow with `failure_condition`.
- A failed HTTP poll or assertion terminates with `step_failed`.
- Each wait uses a 100 ms interval, at most 3,000 iterations, and a 300,000 ms
  deadline. Exhaustion fails with `max_iterations` or `timeout`; it never counts
  as successful completion. Large imports may require increasing these YAML bounds.

Initial deletion of `books` or `hackernews` also works on a fresh database. For
these two cleanup operations only, a failed task triggers a conditional request
that asserts `$.error.code == index_not_found`. Other errors and canceled tasks
still fail. This exception does not apply to normal mutations or final cleanup.

The shell and Rust test harness only wait for server readiness. They no longer
poll the global task queue, so unrelated queued work cannot hold up these flows.
The CLI reports each loop's iteration count, elapsed time, and termination reason.

When using `postman-g` directly against your own server, run dependent files in
order: `documents_ingestion` before `search_and_ranking`, `hackernews_streaming`
before `hackernews_query`, and both ingestion files before `multi_search_and_facets`.
The latter deletes both indexes. `documents_ingestion` expects `movies` not to
exist initially. Avoid passing the whole directory directly to the CLI, which
sorts files alphabetically rather than by fixture dependencies.

## Verification

```bash
# Actual Docker/local server; build postman-g first as above.
cargo test --locked -p postman-flow-e2e --test meilisearch_e2e -- --nocapture

# Deterministic task failure, cancellation, deadline, and cleanup cases; no Docker.
cargo test --locked -p postman-flow-e2e --test meilisearch_task_waits
```

The real-server tests also repeat imports and submit valid NDJSON without the
required `id`, verifying that a later task failure stops a flow even though the
upload itself returned HTTP 202. Local real-server tests print a skip message if
no server can be started. Set `MEILISEARCH_E2E_REQUIRED=1` to make an unavailable
server fail the tests instead. The dedicated CI job always enables this mode;
the general macOS workspace job keeps the optional behavior because it has no Docker.

## GitHub CI

The `Meilisearch E2E Tests` workflow runs on relevant pull requests and pushes to
`main`, `master`, and `develop`, and can also be dispatched manually. It builds
`postman-g` from the checkout, runs CLI/report and deterministic polling tests,
then executes all seven flow files through the Rust E2E scenarios. Each server
scenario has its own container; repeated imports and a background failure after
HTTP 202 are also covered. CI uses `getmeili/meilisearch:v1.53.2` rather than a
moving `latest` tag, and fails if Docker, image preparation, or server startup fails.

To reproduce the strict server tests locally:

```bash
cargo build --locked -p postman-cli --bin postman-g
MEILISEARCH_E2E_REQUIRED=1 \
MEILISEARCH_IMAGE=getmeili/meilisearch:v1.53.2 \
MEILISEARCH_E2E_ARTIFACT_DIR=/tmp/meilisearch-e2e-artifacts \
POSTMAN_G="$PWD/target/debug/postman-g" \
cargo test --locked -p postman-flow-e2e --test meilisearch_e2e -- --nocapture --test-threads=1
```

`MEILISEARCH_E2E_ARTIFACT_DIR` is optional. It saves one JSON report and stderr log
per flow invocation, including failures, plus Docker startup output and server
logs before container removal. Repeated flows use distinct filenames. CI uploads
these files and build/test logs on both success and failure as
`meilisearch-e2e-<run attempt>`, retained for seven days. A final cleanup step also
collects logs from containers left behind by interrupted tests.

Server logs are collected from containers owned by the harness. If using
`MEILISEARCH_URL` or a local binary, collect that server's logs separately. Failed
startup has no flow JSON yet; use the startup/service and test logs instead.
