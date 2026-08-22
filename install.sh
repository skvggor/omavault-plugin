#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

PLUGIN_ID="skvggor.omavault"
PLUGIN_DIR="$HOME/.config/omarchy/plugins/$PLUGIN_ID"

if ! command -v gocryptfs >/dev/null 2>&1; then
  echo "error: gocryptfs is not installed (omarchy pkg add gocryptfs fuse3)" >&2
  exit 1
fi

if command -v cargo >/dev/null 2>&1; then
  cargo build --release
else
  echo "Rust toolchain not found; the prebuilt helper will be fetched after install."
fi

mkdir -p "$PLUGIN_DIR"
cp manifest.json Model.js Panel.qml Service.qml VaultHero.qml VaultIcon.qml setup-helper.sh "$PLUGIN_DIR/"

if command -v cargo >/dev/null 2>&1; then
  cp target/release/omavault-helper "$PLUGIN_DIR/omavault-helper"
else
  bash "$PLUGIN_DIR/setup-helper.sh"
fi

chmod +x "$PLUGIN_DIR/omavault-helper"

omarchy-shell shell rescanPlugins || true

echo "Installed to $PLUGIN_DIR"
echo "Enable with: omarchy plugin enable $PLUGIN_ID"
