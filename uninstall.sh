#!/bin/sh
set -e

INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
CONFIG_DIR="${HOME}/.config/argus"

if [ ! -f "$INSTALL_DIR/argus" ]; then
    echo "argus is not installed at $INSTALL_DIR/argus"
    exit 0
fi

rm -f "$INSTALL_DIR/argus"
echo "Removed $INSTALL_DIR/argus"

if [ -d "$CONFIG_DIR" ]; then
    printf "Remove config at %s? [y/N] " "$CONFIG_DIR"
    read -r answer
    if [ "$answer" = "y" ] || [ "$answer" = "Y" ]; then
        rm -rf "$CONFIG_DIR"
        echo "Removed $CONFIG_DIR"
    fi
fi

echo "argus uninstalled."
