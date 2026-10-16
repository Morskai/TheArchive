import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'

test('main window can receive tray fallback confirmation and release listeners', () => {
  const capability = JSON.parse(readFileSync(new URL('../src-tauri/capabilities/default.json', import.meta.url), 'utf8'))
  assert.deepEqual(capability.windows, ['main', 'player'])
  for (const command of ['listen', 'unlisten']) {
    const permission = `core:event:allow-${command}`
    assert.ok(capability.permissions.includes(permission), `missing ${permission}: close confirmation cannot arrive`)
  }
})
