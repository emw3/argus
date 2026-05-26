#!/bin/sh
set -e

REPO="${ARGUS_REPO:-https://github.com/emw3/argus}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
VERSION="${VERSION:-latest}"

detect_target() {
    OS=$(uname -s)
    if [ "$OS" != "Linux" ]; then
        echo "argus currently supports Linux only (detected: $OS)" >&2
        exit 1
    fi

    ARCH=$(uname -m)
    case $ARCH in
        x86_64) echo "x86_64-unknown-linux-musl" ;;
        aarch64|arm64) echo "aarch64-unknown-linux-musl" ;;
        *) echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
    esac
}

main() {
    TARGET=$(detect_target)

    if [ "$VERSION" = "latest" ]; then
        URL="$REPO/releases/latest/download/argus-${TARGET}.tar.gz"
    else
        URL="$REPO/releases/download/v${VERSION}/argus-${TARGET}-v${VERSION}.tar.gz"
    fi

    echo "Installing argus for ${TARGET}..."
    TMP=$(mktemp -d)
    trap 'rm -rf "$TMP"' EXIT

    if ! curl -sSfL "$URL" -o "$TMP/argus.tar.gz" 2>/dev/null; then
        echo "Download failed. Check $REPO/releases" >&2
        exit 1
    fi

    tar -xzf "$TMP/argus.tar.gz" -C "$TMP"
    install -m 755 "$TMP/argus" "$INSTALL_DIR/argus"

    echo "argus installed to $INSTALL_DIR/argus"
    echo "Run: argus"
}

main
