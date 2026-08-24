# Request History Snapshot V1

`HistoryEntry` is the in-memory projection used by the application. A
`VersionedHistorySnapshot::V1(HistorySnapshotV1)` is the persistence contract. Only the latter may
cross the future `HistoryRepository` boundary. It contains no GPUI entities, response body,
cookie jar, tab state, pane state, dirty state, pending request ID, cancellation handle, download
path, or copied multipart file bytes.

This contract intentionally does not choose a SQLite schema or perform application startup/send
writes. Those belong to issues #129 and #130.

## Envelope and required fields

Serialized JSON uses an envelope with a numeric `version` and a `snapshot` payload. V1 requires:

- stable UUID History entry ID and RFC 3339 timestamp;
- display name, HTTP status, elapsed milliseconds, and response size;
- method and a credential-free absolute HTTP(S) base URL;
- ordered, duplicate-preserving enabled query pairs and non-sensitive headers;
- an explicitly tagged body variant: none, JSON, raw, URL-encoded, or multipart;
- request options: nullable `timeout_ms`, `redirect_policy`, and `max_hops`;
- multipart file paths plus optional filename and content type, never file bytes.

`editor_intent` is nullable. V1 currently uses it for multipart editor rows; empty placeholder rows
are discarded, while meaningful disabled or incomplete rows remain editor-only and never become
enabled transport data. Multipart filename and content type are also nullable.

Unknown envelope versions, malformed enum tags, missing required V1 fields, invalid HTTP fields,
numeric overflow, and invalid option ranges return `HistorySnapshotError`; none may panic.

## Sensitive-data policy

Redaction/removal happens while converting `HistoryEntry` to V1, before a repository can inspect
the value. Decoded V1 payloads are sanitized again before they are returned to callers.

- Header names are matched case-insensitively and separator-insensitively.
- `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie`, known API-key headers, and common
  token/secret/password/credential headers are removed with their values.
- URL usernames and passwords are removed. URL fragments are discarded because they are not sent
  on the wire.
- Common secret-like query names—including API keys, access/auth/bearer/refresh tokens, passwords,
  client secrets, credentials, and signatures—are removed. Safe duplicate query pairs retain
  their order.
- A URL-derived History name is regenerated from the sanitized URL. Known removed values are also
  redacted from a custom name so metadata cannot act as an alternate credential field.
- Transport cookie-jar state and managed bearer/basic fields have no snapshot fields. Their
  effective `Authorization`/`Cookie` headers are removed by the same policy.

Request bodies are user-authored replay data and are persisted verbatim. Arbitrary secrets inside
JSON, raw text, URL-encoded fields, multipart text, file paths, or custom names cannot be inferred
perfectly. The same limitation applies to URL paths and values placed under unrecognized custom
header or query names. Users must therefore treat local History storage as containing authored
request data. The application must never copy a known auth model or cookie jar into a body field.

## Request options and migration defaults

V1 reserves both redirect policies and stores the hop limit now, so issue #68 does not require a
snapshot-version change. Current runtime defaults are `Follow` and 10 hops; timeout `null` means
disabled. Valid hop limits are 1 through 100.

A valid V1 payload contains every required field above. A future migration from an older schema
may construct V1 using these explicit rules before normal V1 validation:

- derive a display name from the sanitized URL when no name exists;
- deterministically synthesize a UUID from the legacy row identity;
- default missing elapsed time or response size metadata to zero;
- default missing request options to timeout disabled, Follow, and 10 hops;
- use no editor intent when the old format did not preserve it;
- require a trustworthy completion status and ordering timestamp—skip and report a legacy row
  when either cannot be recovered;
- require a body kind unless the legacy format proves the request had no body.

Migration must apply the same sensitive-data policy before producing V1. Unsupported newer
versions must remain typed errors and must never be destructively rewritten.

## Multipart replay validation

Snapshots retain only UTF-8 file-path metadata. Startup hydration may still show a recovered row
whose local file moved or disappeared. `validate_replay_files` reports
`HistorySnapshotError::MissingMultipartFile` on replay; it does not embed bytes, silently omit the
part, or crash.
