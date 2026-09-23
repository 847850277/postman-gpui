#!/usr/bin/env bash
set -eo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$DIR/../../../.." && pwd)"

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

# 2. Select flows. Query-only cases need fixtures on the fresh server.
PORT="${PORT:-7700}"
MASTER_KEY="${MEILISEARCH_MASTER_KEY:-masterKey12345678901234567890}"
CONTAINER_NAME="postman_flow_e2e_meili_${PORT}_$$_${RANDOM}"
DATASET="$DIR/data/hackernews_sample.ndjson"
TARGET_PATH=""
PASS_ARGS=()
CHECK_ONLY=false
while [ "$#" -gt 0 ]; do
    case "$1" in
        --input|--var|--timeout-ms)
            if [ "$#" -lt 2 ]; then
                echo "Error: $1 requires a value" >&2
                exit 1
            fi
            PASS_ARGS+=("$1" "$2")
            shift 2
            ;;
        --check)
            CHECK_ONLY=true
            shift
            ;;
        --json|--no-follow-redirects|-v|--verbose)
            PASS_ARGS+=("$1")
            shift
            ;;
        -*)
            echo "Error: unsupported option: $1" >&2
            exit 1
            ;;
        *)
            if [ -n "$TARGET_PATH" ]; then
                echo "Error: specify only one flow file or the flows directory" >&2
                exit 1
            fi
            if [ -e "$1" ]; then
                TARGET_PATH="$1"
            elif [ -e "$DIR/$1" ]; then
                TARGET_PATH="$DIR/$1"
            else
                echo "Error: target does not exist: $1" >&2
                exit 1
            fi
            shift
            ;;
    esac
done

FLOW_FILES=()
if [ -n "$TARGET_PATH" ] && [ -f "$TARGET_PATH" ]; then
    case "$(basename "$TARGET_PATH")" in
        search_and_ranking.http.yml)
            FLOW_FILES+=("$DIR/flows/documents_ingestion.http.yml") ;;
        hackernews_query.http.yml)
            FLOW_FILES+=("$DIR/flows/hackernews_streaming.http.yml") ;;
        multi_search_and_facets.http.yml)
            FLOW_FILES+=("$DIR/flows/documents_ingestion.http.yml" "$DIR/flows/hackernews_streaming.http.yml") ;;
    esac
    FLOW_FILES+=("$TARGET_PATH")
else
    if [ -n "$TARGET_PATH" ]; then
        TARGET_DIR="$(cd "$TARGET_PATH" && pwd)"
        if [ "$TARGET_DIR" != "$DIR/flows" ] && [ "$TARGET_DIR" != "$DIR" ]; then
            echo "Error: directory target must be $DIR/flows or $DIR" >&2
            exit 1
        fi
    fi
    for name in api_keys documents_ingestion search_and_ranking documents_crud_lifecycle hackernews_streaming hackernews_query multi_search_and_facets; do
        FLOW_FILES+=("$DIR/flows/$name.http.yml")
    done
fi

# Reject invalid YAML or an older CLI without loop support before starting Docker.
"$POSTMAN_G" run "${FLOW_FILES[@]}" --check "${PASS_ARGS[@]}" >&2
if [ "$CHECK_ONLY" = true ]; then
    exit 0
fi

cleanup() {
    echo "==> Removing test Meilisearch container ($CONTAINER_NAME)..." >&2
    docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# A busy port is an error; never kill another process or container to free it.
echo "==> Starting ephemeral Meilisearch instance on port $PORT..." >&2
docker run -d --rm \
    --name "$CONTAINER_NAME" \
    -p "127.0.0.1:$PORT:7700" \
    -e "MEILI_MASTER_KEY=$MASTER_KEY" \
    -e "MEILI_NO_ANALYTICS=true" \
    -e "MEILI_ENV=development" \
    "${MEILISEARCH_IMAGE:-getmeili/meilisearch:latest}" >/dev/null

# Only service readiness belongs in the runner. Task completion lives in YAML.
echo "==> Waiting for Meilisearch to be healthy..." >&2
READY=false
for ((i = 0; i < 100; i++)); do
    if curl -fsS --connect-timeout 1 --max-time 1 "http://127.0.0.1:$PORT/health" 2>/dev/null | grep -q 'available'; then
        READY=true
        break
    fi
    sleep 0.1
done
if [ "$READY" != true ]; then
    echo "Error: Meilisearch failed to become ready" >&2
    docker logs "$CONTAINER_NAME" >&2
    exit 1
fi

# Preserve dependency order; the CLI sorts files when passed a directory.
for flow in "${FLOW_FILES[@]}"; do
    echo "==> Running $(basename "$flow")..." >&2
    "$POSTMAN_G" run "$flow" \
        --input host="http://127.0.0.1:$PORT" \
        --input master_key="$MASTER_KEY" \
        --input dataset_path="$DATASET" \
        "${PASS_ARGS[@]}"
done
