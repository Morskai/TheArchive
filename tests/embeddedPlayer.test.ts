import test from 'node:test'
import assert from 'node:assert/strict'
import { playerTime, createPlayerReader, playerKeyAction, adjacentVideoId } from '../src/data/playerState.ts'
import { verifyPlayer } from '../scripts/prepare-player.mjs'
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'

test('player clock handles missing, negative, long and fractional media durations', () => {
  assert.equal(playerTime(0), '00:00')
  assert.equal(playerTime(-1), '00:00')
  assert.equal(playerTime(Number.NaN), '00:00')
  assert.equal(playerTime(65.9), '01:05')
  assert.equal(playerTime(7325), '2:02:05')
  assert.equal(playerTime(12, 7200), '0:00:12')
  assert.equal(playerTime(3600, 7200), '1:00:00')
})
test('old polling replies cannot restore a closed player or replace a new selection', async () => {
  let resolveOld!: (value: number | null) => void
  let resolveNew!: (value: number | null) => void
  const reads = [new Promise<number | null>(resolve => { resolveOld = resolve }), new Promise<number | null>(resolve => { resolveNew = resolve })]
  const applied: (number | null)[] = []
  const refresh = createPlayerReader(() => reads.shift()!, value => applied.push(value))
  const old = refresh(), current = refresh()
  resolveNew(null); await current
  resolveOld(12); await old
  assert.deepEqual(applied, [null])
})

test('opening presentation invalidates an older pending read without waiting for engine loading', async () => {
  let reply!: (value: number | null) => void
  const applied: (number | null)[] = []
  const refresh = createPlayerReader(() => new Promise<number | null>(resolve => { reply = resolve }), value => applied.push(value))
  const old = refresh()
  refresh.invalidate()
  applied.push(2)
  reply(null); await old
  assert.deepEqual(applied, [2])
})
test('packaging rejects a missing or tampered pinned engine', () => {
  const home = mkdtempSync(path.join(tmpdir(), 'player-runtime-'))
  try {
    const file = path.join(home, 'engine.dll')
    const emptyHash = 'E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855'
    assert.throws(() => verifyPlayer(file, emptyHash), /checksum/)
    writeFileSync(file, '')
    assert.doesNotThrow(() => verifyPlayer(file, emptyHash))
    writeFileSync(file, 'tampered')
    assert.throws(() => verifyPlayer(file, emptyHash), /checksum/)
  } finally { rmSync(home, { recursive: true }) }
})
test('player has the fixed pause, fullscreen and twenty-second keyboard actions only', () => {
  assert.deepEqual(playerKeyAction('Space'), { action: 'pause' })
  assert.deepEqual(playerKeyAction('Enter'), { action: 'fullscreen' })
  assert.deepEqual(playerKeyAction('NumpadEnter'), { action: 'fullscreen' })
  assert.deepEqual(playerKeyAction('ArrowLeft'), { action: 'skip', value: '-20' })
  assert.deepEqual(playerKeyAction('ArrowRight'), { action: 'skip', value: '20' })
  assert.equal(playerKeyAction('KeyF'), null)
  assert.deepEqual(playerKeyAction('Backspace'), { action: 'back' })
})
test('previous and next stay within this card and never wrap or guess a missing identity', () => {
  const videos = [{ id: 'first' }, { id: 'middle' }, { id: 'last' }]
  assert.equal(adjacentVideoId(videos, 'middle', -1), 'first')
  assert.equal(adjacentVideoId(videos, 'middle', 1), 'last')
  assert.equal(adjacentVideoId(videos, 'first', -1), null)
  assert.equal(adjacentVideoId(videos, 'last', 1), null)
  assert.equal(adjacentVideoId(videos, 'missing', 1), null)
  assert.equal(adjacentVideoId([], 'missing', -1), null)
})
