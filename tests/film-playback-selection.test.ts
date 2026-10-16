import assert from 'node:assert/strict'
import test from 'node:test'
import { defaultFilmVideo } from '../src/data/realFilmPresentation.ts'

test('library right-click version agrees with detail default and excludes stale preferred versions', () => {
  const videos = [{ id: 'first' }, { id: 'preferred' }]
  assert.equal(defaultFilmVideo({ videos, preferred_video_id: 'preferred' }), 'preferred')
  assert.equal(defaultFilmVideo({ videos, preferred_video_id: 'old' }), 'first')
  assert.equal(defaultFilmVideo({ videos: [] }), null)
})
