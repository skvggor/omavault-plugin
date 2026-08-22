#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

PLUGIN_ID="skvggor.omavault"
PLUGIN_DIR="$HOME/.config/omarchy/plugins/$PLUGIN_ID"
RELEASES_URL="https://github.com/skvggor/omavault-plugin/releases/download"
HELPER_ASSET="omavault-helper-x86_64-linux-musl"

fail() {
  echo "error: $*" >&2
  exit 1
}

download_helper() {
  local version asset_dir

  command -v jq >/dev/null 2>&1 || fail "jq is required to read the plugin version (omarchy pkg add jq)"
  command -v curl >/dev/null 2>&1 || fail "curl is required to download the prebuilt helper (omarchy pkg add curl)"

  if [[ "$(uname -m)" != "x86_64" ]]; then
    fail "no prebuilt helper for $(uname -m); install Rust and re-run (omarchy pkg add rust)"
  fi

  version="$(jq -r .version manifest.json)"
  asset_dir="$(mktemp -d)"
  trap 'rm -rf "$asset_dir"' EXIT

  echo "Rust toolchain not found; downloading prebuilt omavault-helper v$version..."

  if ! curl --fail --location --retry 3 --silent --show-error "$RELEASES_URL/v$version/$HELPER_ASSET" -o "$asset_dir/$HELPER_ASSET"; then
    fail "could not download $RELEASES_URL/v$version/$HELPER_ASSET — no release for v$version yet? Install Rust and re-run instead: omarchy pkg add rust"
  fi
  if ! curl --fail --location --retry 3 --silent --show-error "$RELEASES_URL/v$version/$HELPER_ASSET.sha256" -o "$asset_dir/$HELPER_ASSET.sha256"; then
    fail "could not download $HELPER_ASSET.sha256 from the release"
  fi

  (cd "$asset_dir" && sha256sum --check --status "$HELPER_ASSET.sha256") || fail "checksum mismatch for the downloaded omavault-helper; do not install it"

  mkdir -p target/release
  cp "$asset_dir/$HELPER_ASSET" target/release/omavault-helper
}

if ! command -v gocryptfs >/dev/null 2>&1; then
  fail "gocryptfs is not installed (omarchy pkg add gocryptfs fuse3)"
fi

if command -v cargo >/dev/null 2>&1; then
  cargo build --release
else
  download_helper
fi

mkdir -p "$PLUGIN_DIR"
cp manifest.json Model.js Panel.qml Service.qml VaultHero.qml VaultIcon.qml "$PLUGIN_DIR/"
cp target/release/omavault-helper "$PLUGIN_DIR/omavault-helper"
chmod +x "$PLUGIN_DIR/omavault-helper"

omarchy-shell shell rescanPlugins || true

echo "Installed to $PLUGIN_DIR"
echo "Enable with: omarchy plugin enable $PLUGIN_ID"
