import assert from 'node:assert/strict'
import test from 'node:test'
import { updateArtworkAvailability } from '../src/data/artworkState.ts'

test('repeated and unrelated artwork reports preserve the list snapshot', () => {
  const films = [{ id: 'film-a', has_cover: true }, { id: 'film-b', has_cover: false }]
  assert.equal(updateArtworkAvailability(films, 'film-a', 'has_cover', true), films)
  assert.equal(updateArtworkAvailability(films, 'actor:a', 'has_cover', true), films)
  assert.equal(updateArtworkAvailability(films, 'film-actor:a', 'has_cover', false), films)
})

test('an artwork status change updates only its object and keeps the prior snapshot intact', () => {
  const actors = [{ id: 'a', hasPortrait: true }, { id: 'b', hasPortrait: undefined }]
  const changed = updateArtworkAvailability(actors, 'b', 'hasPortrait', false)
  assert.notEqual(changed, actors)
  assert.equal(changed[0], actors[0])
  assert.equal(changed[1].hasPortrait, false)
  assert.equal(actors[1].hasPortrait, undefined)
  assert.equal(updateArtworkAvailability(changed, 'b', 'hasPortrait', false), changed)
  assert.equal(updateArtworkAvailability(changed, 'b', 'hasPortrait', true)[1].hasPortrait, true)
})
