# postman-flow-e2e

Real-world, industrial end-to-end integration test suites for [postman-flow](https://github.com/847850277/postman-gpui).

## Overview

Unlike synthetic echo tests (like HTTPBingo), `postman-flow-e2e` tests our Flow engine and `postman-g` CLI against **real-world, open-source production applications** to stress-test and verify:
- Multi-step cryptographic handshakes and authentication (OAuth2 password grant, KDF salt/iteration negotiation).
- Stateful business workflows and cross-step dependency propagation.
- Real-world REST API contracts, headers, status codes, and error models.
- Data masking and sensitive variable protection contracts.

## Suites

### 1. Vaultwarden (Bitwarden API)
Located under `suites/vaultwarden/`:
- **12 Full Workflow Specifications** (`.http.yml`):
  - `login.http.yml`: Prelogin KDF negotiation, account registration, OAuth2 password grant token exchange, authenticated profile verification, negative password rejection.
  - `send.http.yml`: Unauthenticated and password-protected Bitwarden Send creation, access control, validation, and deletion.
  - `cyphers_kdf.http.yml`: PBKDF2 (600,000 / 700,000 iterations) and Argon2id security settings mutation and re-authentication.
  - `secrets_cipher.http.yml`: Vault cipher lifecycle (type 1 login, type 5 SSH key), soft-delete to trash, restore from trash.
  - `collection.http.yml`: Organization collection access control and item management.
  - `organization.http.yml`, `organization_policy.http.yml`, `organization_recovery.http.yml`: Org invites, policy enforcement, recovery keys.
  - `admin.http.yml`, `two_factor.http.yml`, `sso_login.http.yml`, `login_smtp.http.yml`.

### 2. Meilisearch (Search & Indexing Engine API)
Located under `suites/meilisearch/`:
- **Workflow Specifications** (`.http.yml`):
  - `api_keys.http.yml`: API Key creation, scoped action/index permissions, bearer authentication, 403 forbidden checks, metadata patch, deletion and 404 verification.
  - `documents_ingestion.http.yml`: Search index creation (`movies`), batch document ingestion with primary key (`id`), and dynamic filterable/sortable attribute updates.
  - `search_and_ranking.http.yml`: Keyword search, combined boolean filtering with sorting, and query highlighting (`_formatted.title`).
  - `hackernews_streaming.http.yml`: Massive NDJSON dataset ingestion via zero-copy streaming file body (`kind: file`), with primary key setup and filter/search attribute indexing.
  - `hackernews_query.http.yml`: Document count index stats verification, exact milestone story retrieval, and filtered comment author searches.
  - `documents_crud_lifecycle.http.yml`: Index/document creation, partial updates, individual and batch deletion, and cleanup.
  - `multi_search_and_facets.http.yml`: Multi-index search, facets, and final index cleanup.

All 16 asynchronous mutations wait for their own task IDs using YAML `repeat_until`;
background failures and timeouts fail the flow. See the [Meilisearch guide](suites/meilisearch/README.md)
for single-file execution, dependencies, polling limits, and cleanup semantics.

### 3. Qdrant (Vector Search Engine API)
Located under `suites/qdrant/`:
- **Workflow Specifications** (`.http.yml`):
  - `collections_lifecycle.http.yml`: Health/readiness checks (`/readyz`), collection creation with vector dimensions (4D) and distance metrics (`Cosine`, `Dot`), cluster parameters verification, collections enumeration, and cleanup.
  - `points_and_vector_search.http.yml`: Batch point upsert with embeddings and metadata payloads (`city`, `price`, `count`), exact point counting, approximate nearest neighbor (ANN) vector search, vector search with payload exact-match filter (`must: city == London`), and numeric range filter (`price <= 5.0`).
  - `payload_crud_and_cleanup.http.yml`: Point retrieval by ID, dynamic payload modification (setting rating/featured flags), verified payload inspection, point batch deletion by ID array, and collection deletion.

## Running the Suites

### Option 1: Automated Script (Local Development)
Runs an ephemeral Vaultwarden server on dynamic/designated port with in-memory/temp SQLite, executes all flows via `postman-g`, and cleans up on exit:

```bash
# Run all suites
./crates/postman-flow-e2e/suites/vaultwarden/run.sh

# Run a single flow
./crates/postman-flow-e2e/suites/vaultwarden/run.sh crates/postman-flow-e2e/suites/vaultwarden/flows/login.http.yml

# Run all Meilisearch suites
./crates/postman-flow-e2e/suites/meilisearch/run.sh

# Run all Qdrant vector database suites
./crates/postman-flow-e2e/suites/qdrant/run.sh

```

### Option 2: Cargo Integration Test
```bash
cargo test -p postman-flow-e2e --test vaultwarden_e2e -- --nocapture
cargo test -p postman-flow-e2e --test meilisearch_e2e -- --nocapture
cargo test -p postman-flow-e2e --test qdrant_e2e -- --nocapture
```
