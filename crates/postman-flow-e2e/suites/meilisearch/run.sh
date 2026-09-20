#!/usr/bin/env bash
set -eo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$DIR/../../.." && pwd)"

# Ensure common binary directories are in PATH
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

# 1. Locate or install postman-g
if [ -z "${POSTMAN_G:-}" ] || [ ! -x "$POSTMAN_G" ]; then
    if [ -x "$PROJECT_ROOT/target/debug/postman-g" ]; then
        POSTMAN_G="$PROJECT_ROOT/target/debug/postman-g"
    elif [ -x "$PROJECT_ROOT/target/release/postman-g" ]; then
        POSTMAN_G="$PROJECT_ROOT/target/release/postman-g"
    elif command -v postman-g >/dev/null 2>&1; then
        POSTMAN_G="$(command -v postman-g)"
    elif [ -x "$HOME/.cargo/bin/postman-g" ]; then
        POSTMAN_G="$HOME/.cargo/bin/postman-g"
    else
        echo "==> postman-g not found. Installing latest release..."
        curl -fsSL https://raw.githubusercontent.com/847850277/postman-gpui/main/install.sh | bash
        POSTMAN_G="$(command -v postman-g || echo "$HOME/.local/bin/postman-g")"
    fi
fi

# 2. Port, Master Key, and Target argument parsing
PORT="${PORT:-7700}"
MASTER_KEY="${MEILISEARCH_MASTER_KEY:-masterKey12345678901234567890}"
CONTAINER_NAME="postman_flow_e2e_meili_$PORT"
DATASET="$DIR/data/hackernews_sample.ndjson"

TARGET_PATH=""
PASS_ARGS=()
for arg in "$@"; do
    if [[ "$arg" == -* ]]; then
        PASS_ARGS+=("$arg")
    elif [ -z "$TARGET_PATH" ] && [ -e "$arg" ]; then
        TARGET_PATH="$arg"
    else
        PASS_ARGS+=("$arg")
    fi
done

# Clean up any lingering container or process on this port
if command -v lsof >/dev/null 2>&1; then
    lsof -ti :"$PORT" | xargs kill -9 2>/dev/null || true
fi
docker rm -f "$CONTAINER_NAME" 2>/dev/null || true

echo "==> Starting ephemeral Meilisearch instance on port $PORT..."
docker run -d --rm     --name "$CONTAINER_NAME"     -p "$PORT:7700"     -e "MEILI_MASTER_KEY=$MASTER_KEY"     -e "MEILI_NO_ANALYTICS=true"     -e "MEILI_ENV=development"     getmeili/meilisearch:latest >/dev/null

cleanup() {
    echo "==> Stopping test Meilisearch container ($CONTAINER_NAME)..."
    docker stop -t 1 "$CONTAINER_NAME" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# 3. Wait for readiness
echo "==> Waiting for Meilisearch to be healthy on port $PORT..."
READY=false
for i in {1..100}; do
    if curl -s "http://127.0.0.1:$PORT/health" | grep -q 'available'; then
        READY=true
        echo "==> Meilisearch is healthy and ready!"
        break
    fi
    sleep 0.1
done

if [ "$READY" != "true" ]; then
    echo "Error: Meilisearch failed to become ready within 10s" >&2
    exit 1
fi

# 4. If a specific target flow file or path was specified, execute it directly
if [ -n "$TARGET_PATH" ]; then
    echo "==> Running specified flow: $TARGET_PATH..."
    "$POSTMAN_G" run "$TARGET_PATH"         --input host="http://127.0.0.1:$PORT"         --input master_key="$MASTER_KEY"         --input dataset_path="$DATASET"         "${PASS_ARGS[@]}"
    exit 0
fi

wait_tasks_drained() {
    while true; do
        PENDING=$(curl -s -H "Authorization: Bearer $MASTER_KEY" "http://127.0.0.1:$PORT/tasks?statuses=enqueued,processing" | grep -o '"total":[0-9]*' | cut -d: -f2 || echo "0")
        if [ "$PENDING" = "0" ] || [ -z "$PENDING" ]; then
            break
        fi
        sleep 0.05
    done
}

# 5. Default: Execute all flow suites in sequence
echo "==> Running Flow Suite 1: API Keys..."
"$POSTMAN_G" run "$DIR/flows/api_keys.http.yml" --input host="http://127.0.0.1:$PORT" --input master_key="$MASTER_KEY" "$@"

echo "==> Running Flow Suite 2: Movies Documents Ingestion..."
"$POSTMAN_G" run "$DIR/flows/documents_ingestion.http.yml" --input host="http://127.0.0.1:$PORT" --input master_key="$MASTER_KEY" "$@"

wait_tasks_drained

echo "==> Running Flow Suite 3: Movies Search & Ranking..."
"$POSTMAN_G" run "$DIR/flows/search_and_ranking.http.yml" --input host="http://127.0.0.1:$PORT" --input master_key="$MASTER_KEY" "$@"

echo "==> Running Flow Suite 4: HackerNews Streaming Dataset Ingestion (File Body)..."
"$POSTMAN_G" run "$DIR/flows/hackernews_streaming.http.yml" --input host="http://127.0.0.1:$PORT" --input master_key="$MASTER_KEY" --input dataset_path="$DATASET" "$@"

wait_tasks_drained

echo "==> Running Flow Suite 5: HackerNews Search, Filter & Highlighting..."
"$POSTMAN_G" run "$DIR/flows/hackernews_query.http.yml" --input host="http://127.0.0.1:$PORT" --input master_key="$MASTER_KEY" "$@"
