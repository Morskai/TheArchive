import assert from 'node:assert/strict'
import test from 'node:test'
import * as actors from '../src/data/actors.ts'
import type { Actor } from '../src/data/actors'

const rows: Actor[] = [
  { ...actors.demoActors[0], id: 'one', name: '甲', aliases: 'ALIAS', tags: ['精选', '已看'], favorite: true },
  { ...actors.demoActors[1], id: 'two', name: '乙', aliases: '', tags: ['精选'], favorite: false },
  { ...actors.demoActors[2], id: 'three', name: '丙', aliases: '', tags: [], favorite: true },
]
const filters = { query: '', favoritesOnly: false, selectedTags: [] as string[] }

test('missing photo filter intersects existing conditions and shares summary fact', () => {
  const photos = [{ ...rows[0], hasPortrait: false }, { ...rows[1], hasPortrait: false }, { ...rows[2], hasPortrait: true }]
  assert.deepEqual(actors.filterActors(photos, { ...filters, missingPhotosOnly: true }).map(row => row.id), ['one', 'two'])
  assert.deepEqual(actors.filterActors(photos, { ...filters, missingPhotosOnly: true, query: 'alias', favoritesOnly: true, selectedTags: ['精选'] }).map(row => row.id), ['one'])
  assert.deepEqual(actors.filterActors(photos.map(row => ({ ...row, hasPortrait: true })), { ...filters, missingPhotosOnly: true }), [])
  const scope = actors.actorSelectionScope(filters)
  assert.deepEqual(actors.selectedActorIds({ scope, ids: ['one'] }, actors.actorSelectionScope({ ...filters, missingPhotosOnly: true }), photos), [])
})

test('actor filters intersect aliases, favorites and every selected tag without a source filter', () => {
  assert.deepEqual(actors.filterActors(rows, { ...filters, query: 'alias', favoritesOnly: true, selectedTags: ['精选', '已看'] }).map(row => row.id), ['one'])
  assert.deepEqual(actors.filterActors(rows, { ...filters, selectedTags: ['精选', '已看'] }).map(row => row.id), ['one'])
  assert.deepEqual(actors.filterActors(rows, { ...filters, query: rows[0].directoryName }), [])
})

test('selecting all uses the complete filtered snapshot and deduplicates stable ids', () => {
  const many = Array.from({ length: 200 }, (_, index) => ({ ...rows[0], id: `id-${index}` }))
  const visible = actors.filterActors([...many, many[0], rows[1]], { ...filters, favoritesOnly: true })
  const scope = actors.actorSelectionScope({ ...filters, favoritesOnly: true })
  const selected = actors.selectedActorIds({ scope, ids: [...visible.map(row => row.id), 'id-0', 'missing'] }, scope, visible)
  assert.equal(selected.length, 200)
  assert.equal(new Set(selected).size, 200)
  assert.ok(selected.includes('id-199'))
  assert.ok(!selected.includes('two'))
})

test('filter changes clear selection immediately while presentation changes preserve it', () => {
  const scope = actors.actorSelectionScope(filters)
  const selection = { scope, ids: ['one', 'two'] }
  assert.deepEqual(actors.selectedActorIds(selection, scope, [...rows].reverse()), ['one', 'two'])
  for (const changed of [{ ...filters, query: '甲' }, { ...filters, favoritesOnly: true }, { ...filters, selectedTags: ['精选'] }]) {
    assert.deepEqual(actors.selectedActorIds(selection, actors.actorSelectionScope(changed), rows), [])
  }
  assert.deepEqual(actors.selectedActorIds(selection, scope, [rows[0]]), ['one'])
})
