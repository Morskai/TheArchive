import assert from 'node:assert/strict'
import test from 'node:test'
import { displayLibraryPath } from '../src/data/libraryPath.ts'

test('Windows device prefixes are hidden in displayed paths', () => {
  assert.equal(displayLibraryPath('\\\\?\\H:\\Collection\\Category'), 'H:\\Collection\\Category')
  assert.equal(displayLibraryPath('\\\\?\\UNC\\server\\share\\Collection'), '\\\\server\\share\\Collection')
  assert.equal(displayLibraryPath('H:\\Collection'), 'H:\\Collection')
})
