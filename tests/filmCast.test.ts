import assert from 'node:assert/strict'
import test from 'node:test'
import { castSaveMessage, filterCastCandidates } from '../src/data/filmCast.ts'
import { createManualReadGuard } from '../src/data/manualState.ts'

test('cast search excludes existing system and manual actors and searches saved names only', () => {
  const candidates = [{ id: 'system', name: 'Avery', library: 'Source A' }, { id: 'manual', name: 'Mira', library: 'Source B' }, { id: 'available', name: 'MIRA Chen', library: 'Source C' }]
  const cast = [{ id: 'system', name: 'Avery', system: true }, { id: 'manual', name: 'Mira', system: false }]
  assert.deepEqual(filterCastCandidates(candidates, cast, '  mira '), [candidates[2]])
  assert.deepEqual(filterCastCandidates(candidates, cast, ''), [candidates[2]])
  assert.deepEqual(filterCastCandidates(candidates, cast, 'Source C'), [])
  assert.deepEqual(filterCastCandidates([], cast, ''), [])
  assert.equal(candidates.length, 3)
})

test('saved cast survives overlapping stale catalog reads without changing other manual fields', async () => {
  const guard = createManualReadGuard<{ actors: { id: string; name: string; system: boolean }[]; rating: number | null }>()
  let resolve!: (rows: { id: string; actors: { id: string; name: string; system: boolean }[]; rating: number | null }[]) => void
  const read = guard.read(() => new Promise<Parameters<typeof resolve>[0]>(done => { resolve = done }), (rows, patch) => patch(rows))
  const actors = [{ id: 'manual', name: 'Mira', system: false }]
  guard.commit('film', { actors })
  resolve([{ id: 'film', actors: [], rating: 4 }, { id: 'other', actors: [], rating: null }])
  assert.deepEqual(await read, [{ id: 'film', actors, rating: 4 }, { id: 'other', actors: [], rating: null }])
})

test('cast errors distinguish protection failure and unavailable source from retryable save errors', () => {
  assert.match(castSaveMessage('protection_failed'), /保护备份失败/)
  assert.match(castSaveMessage('source_paused'), /来源已停用或移除/)
  assert.match(castSaveMessage('cast_save_failed'), /未保存/)
  assert.match(castSaveMessage('film_inactive'), /检查片库状态/)
  assert.match(castSaveMessage('synthetic'), /未保存，请重试/)
})
