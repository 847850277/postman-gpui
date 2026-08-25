# SQLite Request History

Request History uses the bundled, file-backed SQLite database as its only writable and
authoritative source. The application has no in-memory repository, `:memory:` database, volatile
fallback, or persisted Workspace model.

## Lifecycle

At startup the composition root initializes or migrates SQLite and asynchronously queries the
newest 50 rows. After a completed HTTP response it appends one sanitized snapshot, trims in the
same transaction, queries SQLite again, and replaces the visible History projection. Clear follows
the same rule: clear SQLite first, then query it again. A failed append, load, or clear never
speculatively changes the last successful projection.

Only exchanges that produced an HTTP response are persisted, including non-2xx responses.
Cancellation, timeout, DNS, connection, decoding, validation, and other transport failures do not
create History rows. Request execution and response rendering remain usable when History storage
is unavailable.

Recovered History restores one sanitized request into the active editor. It does not restore
response bodies, cookies, loading or cancellation state, pending IDs, tabs, the active tab, scripts,
or unsent drafts.

## Persisted replay shape

The versioned snapshot contains:

- stable entry ID, timestamp, display name, status, elapsed time, and response size;
- method, credential-free base URL, allowed query rows, and allowed headers;
- raw, JSON, URL-encoded, or multipart request body data;
- multipart file path/name/content-type metadata, but never file bytes;
- multipart editor intent needed to retain meaningful disabled rows;
- replay options such as timeout and redirect policy.

An empty multipart placeholder is not persisted. If a referenced multipart file disappears before
replay, Send reports the ordinary missing-file validation error and does not add another History
row.

## Sensitive-data policy

Header and query names are compared case-insensitively after separators are removed. Known
credential locations are denied before the repository boundary, including Authorization,
Proxy-Authorization, Cookie, Set-Cookie, API-key fields, session fields, tokens, passwords,
credentials, secrets, and common signature suffixes. URL user information and cookie-jar state are
also excluded. Response headers and response bodies are outside the snapshot entirely.

This policy cannot infer the meaning of every arbitrary user-defined query name. A query row whose
name does not match the documented denied-name policy is replay data and is persisted even if its
value looks secret. Rename or remove such a row before Send if it must not enter History.

Request bodies are also explicit, user-authored replay data. Their arbitrary contents are persisted
verbatim; the application does not attempt to classify JSON keys, raw text, form values, or
multipart text as secrets. Known authentication models are never copied into a body by the
application, but a credential manually authored inside a body remains the user's responsibility.

## Verification

Run the deterministic application-level suite without public network access:

```sh
cargo history-persistence
```

It uses a unique temporary directory and real SQLite file for every test, starts the real
`PostmanApp` and local HTTP server, destroys and recreates application instances for restart
coverage, and never touches the production History database. Repository unit tests additionally
cover atomic retention, locks, read-only failures, schema migration, corrupt rows, and future schema
rejection.
