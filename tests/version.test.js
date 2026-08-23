'use strict'

const assert = require('node:assert/strict')
const { test } = require('node:test')
const fs = require('node:fs')
const path = require('node:path')

const root = path.join(__dirname, '..')

function manifestVersion() {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json'), 'utf8'))
  return manifest.version
}

function cargoVersion() {
  const cargo = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8')
  const match = cargo.match(/^version\s*=\s*"([^"]+)"/m)
  assert.ok(match, 'Cargo.toml must declare a version')
  return match[1]
}

function setupScriptUrls() {
  const script = fs.readFileSync(path.join(root, 'setup-helper.sh'), 'utf8')
  return {
    releasesUrl: script.match(/^RELEASES_URL="([^"]+)"/m)?.[1],
    attestationsApi: script.match(/^ATTESTATIONS_API="([^"]+)"/m)?.[1],
    helperAsset: script.match(/^HELPER_ASSET="([^"]+)"/m)?.[1],
  }
}

test('manifest.json and Cargo.toml declare the same version', () => {
  assert.equal(manifestVersion(), cargoVersion())
})

test('versions are plain semver so release tags resolve to download URLs', () => {
  for (const version of [manifestVersion(), cargoVersion()]) {
    assert.match(version, /^\d+\.\d+\.\d+$/, `unexpected version: ${version}`)
  }
})

test('setup-helper.sh downloads from the omavault-plugin release channel', () => {
  const urls = setupScriptUrls()
  assert.equal(
    urls.releasesUrl,
    'https://github.com/skvggor/omavault-plugin/releases/download'
  )
  assert.equal(
    urls.attestationsApi,
    'https://api.github.com/repos/skvggor/omavault-plugin/attestations'
  )
  assert.equal(urls.helperAsset, 'omavault-helper-x86_64-linux-musl')
})
