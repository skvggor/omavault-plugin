#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

RELEASES_URL="https://github.com/skvggor/omavault-plugin/releases/download"
HELPER_ASSET="omavault-helper-x86_64-linux-musl"
asset_dir=""

fail() {
  echo "error: $*" >&2
  exit 1
}

cleanup() {
  [[ -n $asset_dir ]] && rm -rf "$asset_dir"
}

command -v jq >/dev/null 2>&1 || fail "jq is required (omarchy pkg add jq)"
command -v curl >/dev/null 2>&1 || fail "curl is required (omarchy pkg add curl)"

if [[ "$(uname -m)" != "x86_64" ]]; then
  fail "no prebuilt helper for $(uname -m); build it from source instead (omarchy pkg add rust)"
fi

version="$(jq -r .version manifest.json)"
asset_dir="$(mktemp -d)"
trap cleanup EXIT

echo "Downloading omavault-helper v$version..."

if ! curl --fail --location --retry 3 --silent --show-error "$RELEASES_URL/v$version/$HELPER_ASSET" -o "$asset_dir/$HELPER_ASSET"; then
  fail "could not download $HELPER_ASSET v$version — no release yet? Build from source instead: omarchy pkg add rust"
fi
if ! curl --fail --location --retry 3 --silent --show-error "$RELEASES_URL/v$version/$HELPER_ASSET.sha256" -o "$asset_dir/$HELPER_ASSET.sha256"; then
  fail "could not download $HELPER_ASSET.sha256 from the release"
fi

(cd "$asset_dir" && sha256sum --check --status "$HELPER_ASSET.sha256") || fail "checksum mismatch for the downloaded omavault-helper; not installing it"

mv "$asset_dir/$HELPER_ASSET" omavault-helper
chmod +x omavault-helper
echo "omavault-helper v$version installed."
