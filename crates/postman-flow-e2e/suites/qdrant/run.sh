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

# 2. Port and Target argument parsing
PORT="${PORT:-6333}"
CONTAINER_NAME="postman_flow_e2e_qdrant_$PORT"

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

echo "==> Starting ephemeral Qdrant instance on port $PORT..."
docker run -d --rm     --name "$CONTAINER_NAME"     -p "$PORT:6333"     qdrant/qdrant:latest >/dev/null

cleanup() {
    echo "==> Stopping test Qdrant container ($CONTAINER_NAME)..."
    docker stop -t 1 "$CONTAINER_NAME" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# 3. Wait for readiness
echo "==> Waiting for Qdrant to be healthy on port $PORT..."
READY=false
for i in {1..100}; do
    if curl -s "http://127.0.0.1:$PORT/readyz" | grep -q 'ready'; then
        READY=true
        echo "==> Qdrant is healthy and ready!"
        break
    fi
    sleep 0.1
done

if [ "$READY" != "true" ]; then
    echo "Error: Qdrant failed to become ready within 10s" >&2
    exit 1
fi

# 4. If a specific target flow file was specified, execute it directly
if [ -n "$TARGET_PATH" ] && [ -f "$TARGET_PATH" ]; then
    echo "==> Running specified flow file: $TARGET_PATH..."
    "$POSTMAN_G" run "$TARGET_PATH"         --input host="http://127.0.0.1:$PORT"         "${PASS_ARGS[@]}"
    exit 0
fi

# 5. Default: Execute all flow suites in sequence
echo "==> Running Flow Suite 1: Collections Lifecycle..."
"$POSTMAN_G" run "$DIR/flows/collections_lifecycle.http.yml" --input host="http://127.0.0.1:$PORT" "${PASS_ARGS[@]}"

echo "==> Running Flow Suite 2: Points & Vector Search..."
"$POSTMAN_G" run "$DIR/flows/points_and_vector_search.http.yml" --input host="http://127.0.0.1:$PORT" "${PASS_ARGS[@]}"

echo "==> Running Flow Suite 3: Payload CRUD & Cleanup..."
"$POSTMAN_G" run "$DIR/flows/payload_crud_and_cleanup.http.yml" --input host="http://127.0.0.1:$PORT" "${PASS_ARGS[@]}"

