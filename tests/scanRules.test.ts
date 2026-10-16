import assert from 'node:assert/strict'
import test from 'node:test'
import { normalizeIgnoredNames } from '../src/data/scanRules.ts'

test('directory names normalize whitespace and preserve whole names', () => {
  assert.deepEqual(normalizeIgnoredNames([' 写真 ', 'Photos', 'Photos-extra']), ['写真', 'Photos', 'Photos-extra'])
  assert.deepEqual(normalizeIgnoredNames([]), [])
})
test('invalid names and case-insensitive duplicates are rejected', () => {
  for (const names of [['photos', 'PHOTOS'], [''], ['../photos'], ['a/b'], ['a\\b'], ['a*'], ['CON'], ['NUL.txt'], ['name.'], ['a'.repeat(256)], Array(101).fill('photo')]) {
    assert.throws(() => normalizeIgnoredNames(names))
  }
})
