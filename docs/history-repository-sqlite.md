# SQLite History Repository

Issue #129 adds the durable storage adapter behind the application-facing `HistoryRepository`
trait. It does not wire startup hydration, send completion, or Clear History into GPUI; that
lifecycle belongs to issue #130.

## Boundary

```text
GPUI / application lifecycle
            ↓ await
HistoryRepositoryWorker (dedicated blocking thread)
            ↓
HistoryRepository trait
            ↓
SqliteHistoryRepository
```

The repository accepts only the sanitized `VersionedHistorySnapshot` from issue #128. SQL,
`rusqlite::Connection`, transactions, and database paths remain inside the SQLite adapter. Each
operation opens and closes its own connection, so no connection is retained across unrelated UI
or runtime state.

## Production database

`production_history_database_path()` resolves the platform-local application-data directory and
appends `postman-gpui/request-history.sqlite3`. Production and injected paths must be absolute;
the adapter never falls back to the process working directory. Initialization creates missing
parent directories.

Production connections use bundled SQLite, foreign keys, WAL journal mode, full mutex mode, and a
bounded busy timeout. Tests inject paths under a unique `tempfile::TempDir` and never resolve or
open the user's production database.

## Schema and migrations

The current schema is version 2 and is tracked with `PRAGMA user_version`.

- V1 introduced `history_entries`: append sequence, stable entry ID, creation time in epoch
  milliseconds, and the serialized sanitized snapshot.
- V2 adds the explicit snapshot-contract version column.
- `(created_at_ms DESC, sequence DESC)` is indexed for deterministic newest-first loading. The
  sequence breaks ties between equal timestamps.
- Every supported migration runs in one immediate transaction. Initialization is idempotent.
- A newer unsupported schema is rejected before WAL or schema mutations; it is never downgraded.
- The checked-in `tests/fixtures/history-schema-v1.sql` fixture proves that a V1 row survives the
  V2 migration.

## Operation rules

- `load_recent(limit)` scans newest first until it has `limit` valid snapshots. A malformed
  individual row is skipped and returned as `HistoryLoadWarning`; older valid rows still load.
- Row metadata must agree with the snapshot entry ID, timestamp, and contract version. Mismatches
  are skipped and diagnosed like malformed rows.
- `append_and_trim(snapshot, limit)` inserts and enforces retention in one immediate transaction.
  Re-appending the identical stable ID is idempotent; the same ID with different bytes is a typed
  conflict.
- `clear()` deletes every durable History row in one transaction.
- Busy/locked, read-only/I/O, corrupt database, migration, unsupported schema, snapshot, and
  generic database failures remain distinct `HistoryRepositoryError` variants for graceful
  degradation in issue #130.

SQLite contains only the snapshot envelope and its ordering/identity metadata. It contains no
response body, cookie jar, auth model, tabs, panes, drafts, pending IDs, cancellation handles, or
download state. Direct database inspection tests assert that credentials denied by the #128
policy are absent.
