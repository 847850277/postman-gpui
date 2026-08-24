PRAGMA user_version = 1;

CREATE TABLE history_entries (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_id TEXT NOT NULL UNIQUE,
    created_at_ms INTEGER NOT NULL,
    snapshot_json BLOB NOT NULL
) STRICT;

CREATE INDEX history_entries_recent_idx
    ON history_entries(created_at_ms DESC, sequence DESC);

INSERT INTO history_entries (entry_id, created_at_ms, snapshot_json)
VALUES (
    '00000000-0000-4000-8000-000000000129',
    1787529600000,
    CAST('{"version":1,"snapshot":{"entry_id":"00000000-0000-4000-8000-000000000129","timestamp":"2026-08-24T00:00:00+00:00","name":"Migrated V1 row","status":200,"elapsed_ms":12,"response_size":34,"request":{"method":"get","url":"https://example.com/legacy","query":[],"headers":[{"name":"X-Legacy","value":"preserved"}],"body":{"kind":"none"},"editor_intent":null,"options":{"timeout_ms":null,"redirect_policy":"follow","max_hops":10}}}}' AS BLOB)
);
