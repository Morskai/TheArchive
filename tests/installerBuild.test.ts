import assert from 'node:assert/strict'
import test from 'node:test'
import { mkdtempSync, writeFileSync, utimesSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { buildEnvironment, validateArguments, verifyInstaller } from '../scripts/build-installer.mjs'

test('standard entry overrides inherited output and target without changing caller environment', () => {
  const inherited = { CARGO_TARGET_DIR: 'Z:/elsewhere', CARGO_BUILD_TARGET: 'aarch64-pc-windows-msvc', Path: 'C:/tools' }
  const environment = buildEnvironment('D:/film-library-source', inherited)
  assert.equal(environment.CARGO_TARGET_DIR, path.resolve('D:/film-library-source/src-tauri/target'))
  assert.equal(environment.CARGO_BUILD_TARGET, undefined)
  assert.equal(inherited.CARGO_TARGET_DIR, 'Z:/elsewhere')
  assert.equal(inherited.CARGO_BUILD_TARGET, 'aarch64-pc-windows-msvc')
  assert.equal(environment.CARGO_BUILD_JOBS, '1')
})

test('fixed entry accepts the existing command but rejects alternate bundles, debug and target paths', () => {
  assert.doesNotThrow(() => validateArguments([]))
  assert.doesNotThrow(() => validateArguments(['--bundles', 'nsis']))
  for (const args of [['--bundles', 'msi'], ['--debug'], ['--no-bundle'], ['--target', 'x86_64-pc-windows-msvc']]) {
    assert.throws(() => validateArguments(args), /NSIS/)
  }
})

test('old installer cannot be mistaken for this build and fresh bytes have a reproducible fingerprint', () => {
  const root = mkdtempSync(path.join(tmpdir(), 'thearchive-installer-'))
  try {
    const file = path.join(root, 'TheArchive_1.0.0_x64-setup.exe')
    assert.throws(() => verifyInstaller(file, Date.now()))
    writeFileSync(file, 'synthetic installer')
    utimesSync(file, new Date(0), new Date(0))
    assert.throws(() => verifyInstaller(file, Date.now()), /predates/)
    const started = Date.now() - 1000
    utimesSync(file, new Date(), new Date())
    const first = verifyInstaller(file, started)
    const second = verifyInstaller(file, started)
    assert.equal(first.bytes, 19)
    assert.match(first.sha256, /^[0-9A-F]{64}$/)
    assert.equal(first.sha256, second.sha256)
  } finally { rmSync(root, { recursive: true, force: true }) }
})
