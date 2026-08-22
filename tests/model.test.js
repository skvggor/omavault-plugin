'use strict'

const assert = require('node:assert/strict')
const { test } = require('node:test')

const Model = require('../plugin/Model.js')

test('parseStatus returns defaults for empty input', () => {
  const parsed = Model.parseStatus('')
  assert.deepEqual(parsed, Model.defaultStatus())
})

test('parseStatus normalizes helper payload', () => {
  const parsed = Model.parseStatus(JSON.stringify({
    ok: true,
    installed: true,
    initialized: true,
    unlocked: true,
    vaultPath: '/home/user/.local/share/omavault/vault',
    mountPath: '/home/user/.local/share/omavault/Protected Files',
    usedBytes: '2048',
    fileCount: '2',
    files: [{ name: 'a.txt' }, { name: 'b.txt' }]
  }))
  assert.equal(parsed.installed, true)
  assert.equal(parsed.usedBytes, 2048)
  assert.equal(parsed.fileCount, 2)
  assert.equal(parsed.files.length, 2)
})

test('parseStatus coerces missing files to empty array', () => {
  const parsed = Model.parseStatus('{"ok":true,"installed":true}')
  assert.deepEqual(parsed.files, [])
})

test('parseStatus flags invalid JSON', () => {
  const parsed = Model.parseStatus('not json at all')
  assert.equal(parsed.ok, false)
  assert.equal(parsed.lastError, 'Failed to parse vault status')
})

test('parseStatus rejects non-object JSON', () => {
  const parsed = Model.parseStatus('[1, 2, 3]')
  assert.equal(parsed.installed, false)
})

test('parseActionOutput carries recovery key', () => {
  const parsed = Model.parseActionOutput('{"ok":true,"initialized":true,"recoveryKey":"abcd-ef01"}')
  assert.equal(parsed.recoveryKey, 'abcd-ef01')
})

test('parseActionOutput defaults missing recovery key to empty string', () => {
  const parsed = Model.parseActionOutput('{"ok":true}')
  assert.equal(parsed.recoveryKey, '')
})

test('passphraseProblem enforces minimum length', () => {
  assert.equal(Model.passphraseProblem('short'), 'Passphrase must be at least 8 characters')
  assert.equal(Model.passphraseProblem(''), 'Passphrase must be at least 8 characters')
})

test('passphraseProblem accepts long passphrases', () => {
  assert.equal(Model.passphraseProblem('correct horse battery'), '')
})

test('passphraseProblem detects mismatched confirmation', () => {
  assert.equal(Model.passphraseProblem('correct horse', 'different horse'), 'Passphrases do not match')
  assert.equal(Model.passphraseProblem('correct horse', 'correct horse'), '')
})

test('fileExtension and fileKind classify names', () => {
  assert.equal(Model.fileExtension('Photo.JPG'), 'jpg')
  assert.equal(Model.fileKind('Photo.JPG'), 'image')
  assert.equal(Model.fileKind('clip.mp4'), 'video')
  assert.equal(Model.fileKind('notes.md'), 'document')
  assert.equal(Model.fileKind('archive.tar.gz'), 'misc')
  assert.equal(Model.fileKind('no-extension'), 'misc')
})

test('fileGlyph maps kinds to glyphs', () => {
  assert.equal(Model.fileGlyph('a.png'), '󰋩')
  assert.equal(Model.fileGlyph('a.mp4'), '󰈫')
  assert.equal(Model.fileGlyph('a.pdf'), '󰈙')
  assert.equal(Model.fileGlyph('a.bin'), '󰈔')
})

test('formatBytes renders human units', () => {
  assert.equal(Model.formatBytes(0), '0 B')
  assert.equal(Model.formatBytes(999), '999 B')
  assert.equal(Model.formatBytes(1500), '1.5 KB')
  assert.equal(Model.formatBytes(42_000_000), '42 MB')
})

test('formatBytes tolerates invalid input', () => {
  assert.equal(Model.formatBytes(undefined), '0 B')
  assert.equal(Model.formatBytes('nope'), '0 B')
})

test('formatCountdown renders time remaining', () => {
  assert.equal(Model.formatCountdown(0), '0s')
  assert.equal(Model.formatCountdown(45_000), '45s')
  assert.equal(Model.formatCountdown(120_000), '2m 0s')
  assert.equal(Model.formatCountdown(582_500), '9m 42s')
  assert.equal(Model.formatCountdown(3_600_000), '1h 0m 0s')
  assert.equal(Model.formatCountdown(3_912_000), '1h 5m 12s')
})

test('relativeTime renders friendly ages', () => {
  const now = 1_000_000_000_000
  assert.equal(Model.relativeTime(0), 'Unknown time')
  assert.equal(Model.relativeTime(now / 1000, now), 'Just now')
  assert.equal(Model.relativeTime(now / 1000 - 300, now), '5m ago')
  assert.equal(Model.relativeTime(now / 1000 - 7200, now), '2h ago')
  assert.equal(Model.relativeTime(now / 1000 - 86_400, now), '1d ago')
})

test("fileMeta joins age and size", () => {
  const now = 1_000_000_000_000
  const meta = Model.fileMeta({ modifiedTs: now / 1000 - 60, sizeBytes: 2000 }, now)
  assert.equal(meta, '1m ago · 2 KB')
  assert.equal(Model.fileMeta(null), '')
})

test('parseStatus normalizes holders', () => {
  const parsed = Model.parseStatus(JSON.stringify({
    ok: true, unlocked: true,
    holders: [{ process: 'nvim', openPaths: ['/vault/a.md', '/vault/b.md'] }, 'garbage']
  }))
  assert.equal(parsed.holders.length, 2)
  assert.equal(parsed.holders[0].process, 'nvim')
  assert.equal(parsed.holders[0].openPaths.length, 2)
  assert.deepEqual(parsed.holders[1], { process: 'unknown', openPaths: [] })
})

test('parseStatus defaults holders to empty array', () => {
  const parsed = Model.parseStatus('{"ok":true}')
  assert.deepEqual(parsed.holders, [])
})

test('holderSummary lists process and first file name', () => {
  assert.equal(
    Model.holderSummary({ process: 'nvim', openPaths: ['/mount/docs/a.md', '/mount/b.md'] }),
    'nvim · a.md + 1 more'
  )
  assert.equal(Model.holderSummary({ process: 'nautilus', openPaths: ['/mount'] }), 'nautilus · mount')
  assert.equal(Model.holderSummary(null), '')
})

test('parseActionOutput normalizes recoveredCount', () => {
  const parsed = Model.parseActionOutput('{"ok":true,"unlocked":true,"recoveredCount":"2"}')
  assert.equal(parsed.recoveredCount, 2)
  assert.equal(Model.parseActionOutput('{"ok":true}').recoveredCount, 0)
})

test('parseActionOutput normalizes passphraseChanged', () => {
  assert.equal(Model.parseActionOutput('{"ok":true,"passphraseChanged":true}').passphraseChanged, true)
  assert.equal(Model.parseActionOutput('{"ok":true}').passphraseChanged, false)
})

test('parseStatus defaults pendingRecovered to zero', () => {
  assert.equal(Model.parseStatus('{"ok":true}').pendingRecovered, 0)
  assert.equal(Model.parseStatus('{"ok":true,"pendingRecovered":"3"}').pendingRecovered, 3)
})

test('parseActionOutput normalizes restore and discard counts', () => {
  const restored = Model.parseActionOutput('{"ok":true,"restored":2}')
  assert.equal(restored.restored, 2)
  const discarded = Model.parseActionOutput('{"ok":true,"discarded":"5"}')
  assert.equal(discarded.discarded, 5)
  assert.equal(Model.parseActionOutput('{"ok":true}').restored, 0)
  assert.equal(Model.parseActionOutput('{"ok":true}').discarded, 0)
})

test('parseActionOutput surfaces helper failure messages', () => {
  const parsed = Model.parseActionOutput('{"ok":false,"error":"vault already initialized"}')
  assert.equal(parsed.ok, false)
  assert.equal(parsed.lastError, 'vault already initialized')
})

test('parseActionOutput translates wrong passphrase failures', () => {
  const parsed = Model.parseActionOutput(JSON.stringify({
    ok: false,
    error: 'failed to unlock master key: cipher: message authentication failed\nPassword incorrect.'
  }))
  assert.equal(parsed.lastError, 'Incorrect passphrase')
})

test('friendlyError keeps unrelated messages and collapses whitespace', () => {
  assert.equal(Model.friendlyError('failed to read passphrase: boom'), 'failed to read passphrase: boom')
  assert.equal(Model.friendlyError('line one\n   line two'), 'line one line two')
  assert.equal(Model.friendlyError(''), '')
  assert.equal(Model.friendlyError(null), '')
})

test('friendlyError translates invalid recovery key failures', () => {
  assert.equal(
    Model.friendlyError('Could not parse master key: encoding/hex: invalid byte'),
    'Invalid recovery key'
  )
})
