#!/usr/bin/env bash
set -euo pipefail

# Installs postman-g CLI binary for postman-gpui.
REPO="847850277/postman-gpui"

detect_install_dir() {
    if [ -n "${POSTMAN_INSTALL_DIR:-}" ]; then
        echo "$POSTMAN_INSTALL_DIR"
    elif [ -w "/usr/local/bin" ]; then
        echo "/usr/local/bin"
    elif [ -d "$HOME/.cargo/bin" ]; then
        echo "$HOME/.cargo/bin"
    else
        echo "$HOME/.local/bin"
    fi
}

download() {
    local url="$1"
    local dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -sSfL "$url" -o "$dest"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$dest" "$url"
    else
        echo "Error: curl or wget is required to download binaries." >&2
        return 1
    fi
}

main() {
    echo "Installing postman-g..."
    INSTALL_DIR="$(detect_install_dir)"
    mkdir -p "$INSTALL_DIR"

    OS="$(uname -s)"
    ARCH="$(uname -m)"

    ASSET=""
    case "$OS" in
        Darwin)
            ASSET="postman-g-macos-universal.tar.gz"
            ;;
        Linux)
            case "$ARCH" in
                x86_64|amd64)
                    ASSET="postman-g-linux-x86_64.tar.gz"
                    ;;
                aarch64|arm64)
                    ASSET="postman-g-linux-aarch64.tar.gz"
                    ;;
                *)
                    echo "Notice: no prebuilt Linux binary for architecture: $ARCH" >&2
                    ;;
            esac
            ;;
        *)
            echo "Notice: unsupported operating system: $OS" >&2
            ;;
    esac

    INSTALLED=false

    if [ -n "$ASSET" ]; then
        DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/$ASSET"
        TMP_DIR="$(mktemp -d)"
        trap 'rm -rf "$TMP_DIR"' EXIT

        echo "Checking for prebuilt binary from GitHub Releases ($ASSET)..."
        if download "$DOWNLOAD_URL" "$TMP_DIR/$ASSET" 2>/dev/null; then
            tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"
            if [ -f "$TMP_DIR/postman-g" ]; then
                install -m 755 "$TMP_DIR/postman-g" "$INSTALL_DIR/postman-g"
                echo "Downloaded and installed postman-g to $INSTALL_DIR/postman-g"
                INSTALLED=true
            fi
        else
            echo "Prebuilt release binary not available on latest release yet."
        fi
    fi

    if [ "$INSTALLED" = false ]; then
        if command -v cargo >/dev/null 2>&1; then
            echo "Building and installing postman-g from source via cargo..."
            cargo install --git "https://github.com/$REPO" postman-cli --bin postman-g --locked
            INSTALLED=true
        else
            echo "Error: Could not install postman-g." >&2
            echo "Please install Rust/Cargo (https://rustup.rs) or download prebuilt binaries from:" >&2
            echo "https://github.com/$REPO/releases" >&2
            exit 1
        fi
    fi

    echo "Successfully installed postman-g!"
    if ! command -v postman-g >/dev/null 2>&1; then
        echo ""
        echo "Notice: $INSTALL_DIR is not in your PATH."
        echo "Add it to your profile (e.g. ~/.bashrc or ~/.profile):"
        echo "  export PATH="$INSTALL_DIR:$PATH""
    fi
}

main "$@"
