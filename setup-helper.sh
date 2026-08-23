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

# Verify build attestation (required). GitHub signs provenance server-side,
# so an attestation fetched over TLS for this exact digest proves the binary
# was produced by this repository's Actions build — unlike the checksum file,
# which is co-published and falls with the same channel.
verify_attestation() {
  local attestation_file
  local binary_sha256

  echo "Verifying build attestation..."

  binary_sha256=$(sha256sum "$asset_dir/$HELPER_ASSET" | cut -d' ' -f1)

  attestation_file=$(mktemp)
  trap "rm -f '$attestation_file'" RETURN

  # The digest belongs in the path segment; query parameters are ignored.
  # The attestations endpoint intermittently answers 504, hence the retries.
  if ! curl --fail --location --retry 4 --retry-delay 2 --silent --show-error \
    -H "Accept: application/vnd.github+json" \
    -H "User-Agent: omavault-setup" \
    "$ATTESTATIONS_API/sha256:$binary_sha256" \
    -o "$attestation_file"; then
    echo "Could not fetch the build attestation from the GitHub API." >&2
    return 1
  fi

  if ! jq -e '.attestations | length > 0' "$attestation_file" >/dev/null 2>&1; then
    echo "No build attestations exist for this binary's digest." >&2
    return 1
  fi

  # The repository URI lives inside the base64 DSSE payload — the part the
  # envelope actually signs — not in the surrounding metadata.
  if jq -e '.attestations[0].bundle.dsseEnvelope.payload
    | @base64d
    | test("https://github\\.com/skvggor/omavault-plugin")
    and test("https://slsa\\.dev/provenance")' "$attestation_file" >/dev/null 2>&1; then
    echo "✓ Build attestation verified (GitHub-signed provenance)"
    return 0
  fi
  echo "Attestation found but it does not reference skvggor/omavault-plugin." >&2
  return 1
}

if ! verify_attestation; then
  fail "build attestation verification failed for omavault-helper v$version — refusing to install it. Check your network (the GitHub API rate-limits unauthenticated requests) and retry, or verify manually with: gh attestation verify"
fi

mv "$asset_dir/$HELPER_ASSET" omavault-helper
chmod +x omavault-helper
echo "omavault-helper v$version installed."
