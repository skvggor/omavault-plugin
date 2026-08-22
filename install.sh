#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

PLUGIN_ID="skvggor.omavault"
PLUGIN_DIR="$HOME/.config/omarchy/plugins/$PLUGIN_ID"

if ! command -v gocryptfs >/dev/null 2>&1; then
  echo "error: gocryptfs is not installed (sudo pacman -S --needed gocryptfs fuse3)" >&2
  exit 1
fi

cargo build --release

mkdir -p "$PLUGIN_DIR"
cp plugin/manifest.json plugin/Model.js plugin/Panel.qml plugin/Service.qml plugin/VaultHero.qml plugin/VaultIcon.qml "$PLUGIN_DIR/"
cp target/release/omavault-helper "$PLUGIN_DIR/omavault-helper"
chmod +x "$PLUGIN_DIR/omavault-helper"

omarchy-shell shell rescanPlugins || true

echo "Installed to $PLUGIN_DIR"
echo "Enable with: omarchy plugin enable $PLUGIN_ID"
