#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

RELEASES_URL="https://github.com/skvggor/omavault-plugin/releases/download"
ATTESTATIONS_API="https://api.github.com/repos/skvggor/omavault-plugin/attestations"
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

# Verify checksum (required)
(cd "$asset_dir" && sha256sum --check --status "$HELPER_ASSET.sha256") || fail "checksum mismatch for the downloaded omavault-helper; not installing it"

# Optionally verify build attestation via GitHub API
verify_attestation() {
  local attestation_file
  local binary_sha256
  
  echo "Verifying build attestation..."
  
  # Calculate the binary's SHA-256
  binary_sha256=$(sha256sum "$asset_dir/$HELPER_ASSET" | cut -d' ' -f1)
  
  attestation_file=$(mktemp)
  trap "rm -f '$attestation_file'" RETURN
  
  # Query GitHub Attestations API for this binary's digest
  if ! curl --fail --location --retry 2 --silent --show-error \
    -H "Accept: application/vnd.github+json" \
    -H "User-Agent: omavault-setup" \
    "$ATTESTATIONS_API?subject_digest=sha256:$binary_sha256&include=all" \
    -o "$attestation_file" 2>/dev/null; then
    echo "⚠ Could not fetch attestation from GitHub API"
    return 1
  fi
  
  # Check if any attestations were found
  if ! jq -e '.attestations | length > 0' "$attestation_file" >/dev/null 2>&1; then
    echo "⚠ No attestations found for this binary"
    return 1
  fi
  
  # Verify the attestation mentions the expected repository
  if jq -e '.attestations[0].bundle.verificationMaterial.content | test("github\\.com/skvggor/omavault-plugin")' "$attestation_file" >/dev/null 2>&1; then
    echo "✓ Build attestation verified (GitHub-signed provenance)"
    return 0
  else
    echo "⚠ Attestation found but cannot verify repository origin"
    return 1
  fi
}

# Attempt attestation verification, but don't fail if it's unavailable
if ! verify_attestation; then
  echo "Note: Build attestation verification unavailable or failed."
  echo "      The binary has been verified by SHA-256 checksum."
  echo "      For manual verification, see:"
  echo "      https://docs.github.com/en/actions/building-and-testing-github-actions/about-attestations"
fi

mv "$asset_dir/$HELPER_ASSET" omavault-helper
chmod +x omavault-helper
echo "omavault-helper v$version installed."
