#!/usr/bin/env bash
set -euo pipefail

# Both installation paths use one release tag and one final binary directory.
REPO="847850277/postman-gpui"
TMP_DIR=""
STAGED_BINARY=""

fail() {
    echo "Error: $*" >&2
    exit 1
}

cleanup() {
    if [ -n "$STAGED_BINARY" ]; then
        rm -f -- "$STAGED_BINARY"
    fi
    if [ -n "$TMP_DIR" ]; then
        rm -rf -- "$TMP_DIR"
    fi
}

detect_install_dir() {
    if [ -n "${POSTMAN_INSTALL_DIR:-}" ]; then
        printf '%s\n' "$POSTMAN_INSTALL_DIR"
    elif [ -w "/usr/local/bin" ]; then
        echo "/usr/local/bin"
    elif [ -d "$HOME/.cargo/bin" ]; then
        echo "$HOME/.cargo/bin"
    else
        echo "$HOME/.local/bin"
    fi
}

# Save response headers for latest-tag resolution. Only a completed HTTP 404
# returns 44: transport errors and other HTTP failures must never enable fallback.
download() {
    local url="$1" dest="$2" status="" result=0
    if command -v curl >/dev/null 2>&1; then
        status=$(curl -sSL --dump-header "$dest.headers" --output "$dest" \
            --write-out '%{http_code}' "$url") || result=$?
        if [ "$result" -eq 0 ] && [ "$status" = 404 ]; then
            return 44
        fi
    elif command -v wget >/dev/null 2>&1; then
        LC_ALL=C wget --no-verbose --server-response --output-document="$dest" \
            "$url" 2>"$dest.headers" || result=$?
        status=$(awk '/^[[:space:]]*HTTP\/[0-9.]+ [0-9]+/ { code = $2 } END { print code }' "$dest.headers")
        if [ "$result" -eq 8 ] && [ "$status" = 404 ]; then
            return 44
        fi
    else
        echo "Error: curl or wget is required to download releases." >&2
        return 1
    fi

    if [ "$result" -eq 0 ] && [[ "$status" == 2[0-9][0-9] ]]; then
        return 0
    fi
    echo "Error: download failed for $url (HTTP ${status:-unknown}, exit $result)." >&2
    return 1
}

validate_version() {
    # Match the release tooling's vX.Y.Z / vX.Y.Z-prerelease tag contract.
    [[ "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?$ ]] || \
        fail "Invalid release version '$1'; expected vX.Y.Z or vX.Y.Z-prerelease."
}

resolve_version() {
    local url="https://github.com/$REPO/releases" version="${POSTMAN_VERSION:-}"
    if [ -n "$version" ]; then
        validate_version "$version"
        download "$url/tag/$version" "$TMP_DIR/release" || \
            fail "Release $version does not exist or could not be downloaded."
    else
        # GitHub redirects /latest to a concrete release page. Resolve it once,
        # without requiring a JSON parser or consulting /latest again for assets.
        download "$url/latest" "$TMP_DIR/release" || \
            fail "Could not resolve the latest release."
        version=$(awk -v prefix="$url/tag/" '
            tolower($1) == "location:" {
                location = $2
                sub(/\r$/, "", location)
                if (index(location, prefix) == 1) tag = substr(location, length(prefix) + 1)
            }
            END { print tag }
        ' "$TMP_DIR/release.headers")
        [ -n "$version" ] || fail "Latest release did not resolve to a release tag."
        validate_version "$version"
    fi
    printf '%s\n' "$version"
}

verify_archive() {
    local asset="$1" expected actual
    # Release manifests use ./filename; accept standard text/binary checksum
    # markers too. Reject duplicate, missing, or malformed entries for this asset.
    expected=$(awk -v asset="$asset" '
        {
            name = $2
            sub(/^\*/, "", name)
            sub(/^\.\//, "", name)
            if (name == asset) {
                count++
                if (NF != 2 || length($1) != 64 || $1 ~ /[^0-9a-fA-F]/) invalid = 1
                hash = tolower($1)
            }
        }
        END {
            if (count != 1 || invalid) exit 1
            print hash
        }
    ' "$TMP_DIR/SHA256SUMS") || fail "Missing or invalid SHA256SUMS entry for $asset."

    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$TMP_DIR/$asset") || fail "Could not calculate SHA256 for $asset."
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$TMP_DIR/$asset") || fail "Could not calculate SHA256 for $asset."
    else
        fail "sha256sum or shasum is required to verify the archive."
    fi
    [ "${actual%% *}" = "$expected" ] || fail "SHA256 mismatch for $asset; refusing to install."
    echo "Verified SHA256 for $asset."
}

install_binary() {
    local source="$1" install_dir="$2"
    [ -f "$source" ] && [ ! -L "$source" ] || fail "Expected postman-g executable is missing or invalid."
    mkdir -p -- "$install_dir" || fail "Could not create installation directory $install_dir."
    [ ! -d "$install_dir/postman-g" ] || fail "$install_dir/postman-g is a directory."

    # Stage on the destination filesystem, then replace atomically so failures
    # (including a failed Cargo build) leave an existing executable untouched.
    STAGED_BINARY=$(mktemp "$install_dir/.postman-g.XXXXXX") || fail "Could not stage the executable in $install_dir."
    install -m 755 "$source" "$STAGED_BINARY" || fail "Could not stage the postman-g executable."
    mv -f -- "$STAGED_BINARY" "$install_dir/postman-g" || fail "Could not install $install_dir/postman-g."
    STAGED_BINARY=""
}

main() {
    local install_dir version os arch asset="" download_url result source
    echo "Installing postman-g..."
    install_dir="$(detect_install_dir)"
    # Normalize relative destinations before passing them to filesystem tools.
    case "$install_dir" in
        /*) ;;
        *) install_dir="$PWD/$install_dir" ;;
    esac
    TMP_DIR=$(mktemp -d) || fail "Could not create a temporary directory."
    trap cleanup EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    version=$(resolve_version)
    echo "Selected release $version."

    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Darwin) asset="postman-g-macos-universal.tar.gz" ;;
        Linux)
            case "$arch" in
                x86_64|amd64) asset="postman-g-linux-x86_64.tar.gz" ;;
                aarch64|arm64) asset="postman-g-linux-aarch64.tar.gz" ;;
            esac
            ;;
    esac

    if [ -n "$asset" ]; then
        download_url="https://github.com/$REPO/releases/download/$version"
        echo "Downloading $asset from release $version..."
        if download "$download_url/$asset" "$TMP_DIR/$asset"; then
            download "$download_url/SHA256SUMS" "$TMP_DIR/SHA256SUMS" || \
                fail "Could not download SHA256SUMS for $version; refusing to install."
            verify_archive "$asset"
            mkdir "$TMP_DIR/unpacked"
            tar -xzf "$TMP_DIR/$asset" -C "$TMP_DIR/unpacked" postman-g || \
                fail "Could not extract postman-g from the verified archive."
            source="$TMP_DIR/unpacked/postman-g"
        else
            result=$?
            [ "$result" -eq 44 ] || fail "Could not download $asset for $version; refusing source fallback."
            echo "No prebuilt $asset for $version; building the same release from source."
        fi
    else
        echo "No prebuilt binary for $os/$arch; building $version from source."
    fi

    if [ -z "${source:-}" ]; then
        command -v cargo >/dev/null 2>&1 || fail "Rust/Cargo is required to build release $version (https://rustup.rs)."
        # --root is a Cargo prefix, not the requested binary directory. Keep
        # Cargo's bin/ and metadata temporary and install only the final binary.
        cargo install --git "https://github.com/$REPO" --tag "$version" \
            postman-cli --bin postman-g --locked --root "$TMP_DIR/cargo" || \
            fail "Cargo failed to build release $version."
        source="$TMP_DIR/cargo/bin/postman-g"
    fi

    install_binary "$source" "$install_dir"
    echo "Successfully installed postman-g $version to $install_dir/postman-g"
    case ":$PATH:" in
        *":$install_dir:"*) ;;
        *)
            echo "Notice: $install_dir is not in your PATH."
            echo "Add it to your profile (e.g. ~/.bashrc or ~/.profile):"
            printf '  export PATH=%q:"$PATH"\n' "$install_dir"
            ;;
    esac
}

main "$@"
