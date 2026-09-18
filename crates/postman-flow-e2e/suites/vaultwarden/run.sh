#!/usr/bin/env bash
set -eo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$DIR/../../../.." && pwd)"

# Find postman-g
POSTMAN_G="${POSTMAN_G:-$(which postman-g 2>/dev/null || echo "$REPO_ROOT/target/debug/postman-g")}"

if [ ! -x "$POSTMAN_G" ]; then
    echo "==> Building postman-g..."
    cargo build --manifest-path "$REPO_ROOT/Cargo.toml" -p postman-cli --bin postman-g
fi

PORT="${VAULTWARDEN_PORT:-8088}"
CONTAINER_NAME="postman_flow_e2e_vw_$PORT"

# Parse arguments: detect if first argument is a path or an option
TARGET_PATH="$DIR/flows"
PASS_ARGS=()

for arg in "$@"; do
    if [[ "$arg" == -* ]]; then
        PASS_ARGS+=("$arg")
    elif [ "$TARGET_PATH" = "$DIR/flows" ] && [ -e "$arg" ]; then
        TARGET_PATH="$arg"
    else
        PASS_ARGS+=("$arg")
    fi
done

# If VAULTWARDEN_URL is set directly, use it without spinning up local container
if [ -n "${VAULTWARDEN_URL:-}" ]; then
    echo "==> Using external Vaultwarden URL: $VAULTWARDEN_URL"
    "$POSTMAN_G" run "$TARGET_PATH" --input host="$VAULTWARDEN_URL" "${PASS_ARGS[@]}"
    exit 0
fi

# If docker is available, run official vaultwarden/server container
if command -v docker >/dev/null 2>&1; then
    echo "==> Starting ephemeral Vaultwarden container ($CONTAINER_NAME) on port $PORT..."
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
    docker run -d --rm --name "$CONTAINER_NAME" -p "$PORT:80" \
        -e I_REALLY_WANT_VOLATILE_STORAGE=true \
        -e SIGNUPS_ALLOWED=true \
        -e WEB_VAULT_ENABLED=false \
        -e LOG_LEVEL=warn \
        -e LOGIN_RATELIMIT_MAX_BURST=1000 \
        -e UNAUTHENTICATED_RATELIMIT_MAX_BURST=1000 \
        -e ADMIN_TOKEN=testadmin1234567890 \
        vaultwarden/server:latest >/dev/null

    cleanup() {
        echo "==> Stopping Vaultwarden container..."
        docker stop "$CONTAINER_NAME" >/dev/null 2>&1 || true
    }
    trap cleanup EXIT INT TERM
else
    echo "Error: Docker is required to run ephemeral Vaultwarden or set VAULTWARDEN_URL=http://your-host:port" >&2
    exit 1
fi

echo "==> Waiting for Vaultwarden to be healthy..."
READY=false
for i in {1..60}; do
    if curl -s "http://127.0.0.1:$PORT/alive" >/dev/null 2>&1; then
        READY=true
        echo "==> Vaultwarden is healthy at http://127.0.0.1:$PORT"
        break
    fi
    sleep 0.1
done

if [ "$READY" != "true" ]; then
    echo "Error: Vaultwarden failed to start within 6 seconds" >&2
    exit 1
fi

echo "==> Executing Vaultwarden E2E API flow suites via postman-g..."
"$POSTMAN_G" run "$TARGET_PATH" --input host="http://127.0.0.1:$PORT" "${PASS_ARGS[@]}"
