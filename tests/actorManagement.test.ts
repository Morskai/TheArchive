import assert from 'node:assert/strict'
import test from 'node:test'
import { actorInput, actorEditorError, splitActorAliases, actorManagementError } from '../src/data/actorManagementState.ts'
import { actorManagementCommands } from './helpers/actorManagementCommands.ts'

const profile = { birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' }
const item = { key: 'one', directory: 'Synthetic/Mira', name: 'Mira', aliases: ['Mi'], existingId: 'saved', status: 'changed' as const, changes: ['name', 'photo', 'profile'], reason: null }

test('alias input trims and deduplicates individual aliases without merging actors', () => {
  assert.deepEqual(splitActorAliases(' Mi 、米, Mi， MIRA \n'), ['Mi', '米', 'MIRA'])
  assert.deepEqual(splitActorAliases('  '), [])
})

test('editor payload retains optional photo and explicit unique film ids', () => {
  const input = actorInput({ name: ' Mira ', aliases: 'Mi、米', ...profile, cup: 'b' }, ['film-one', 'film-one', 'film-two'], 'actor-one', null, false)
  assert.deepEqual(input, { actorId: 'actor-one', name: 'Mira', aliases: ['Mi', '米'], profile: { ...profile, cup: 'B' }, photoPath: null, removePhoto: false, filmIds: ['film-one', 'film-two'] })
  assert.equal(actorInput({ name: 'Mira', aliases: '', ...profile }, [], undefined, 'Synthetic/photo.png', false).photoPath, 'Synthetic/photo.png')
})

test('editor rejects invalid values and accepts optional empty fields', () => {
  const draft = { name: 'Mira', aliases: '', ...profile }
  assert.equal(actorEditorError(draft, '2026-10'), '')
  assert.match(actorEditorError({ ...draft, name: ' ' }, '2026-10'), /姓名/)
  assert.match(actorEditorError({ ...draft, height: '99' }, '2026-10'), /100–230/)
  assert.match(actorEditorError({ ...draft, birthMonth: '2026-11' }, '2026-10'), /本月/)
  assert.match(actorEditorError({ ...draft, waist: '40.5' }, '2026-10'), /整数/)
})

test('birth information accepts the supplied year or month without inventing precision', () => {
  const draft = { name: 'Mira', aliases: '', ...profile }
  for (const birthMonth of ['1900', '1993', '2003', '2026', '1900-01', '2026-10']) {
    assert.equal(actorEditorError({ ...draft, birthMonth }, '2026-10'), '', birthMonth)
    assert.equal(actorInput({ ...draft, birthMonth }, [], undefined, null, false).profile.birthMonth, birthMonth)
  }
  for (const birthMonth of ['1899', '2027', '2026-11', '199', '2003-', '2003-00', '2003-13', '2003-1', '2003-01-01', '２００３']) {
    assert.match(actorEditorError({ ...draft, birthMonth }, '2026-10'), /出生/, birthMonth)
  }
})

test('stale and protection failures tell the user what can be retried', () => {
  assert.match(actorManagementError('actor_preview_changed'), /重新选择目录并扫描/)
  assert.match(actorManagementError('protection_failed'), /保护/)
  assert.match(actorManagementError('actor_export_unsafe'), /导出目录/)
  assert.match(actorManagementError('actor_export_photo_in_use'), /覆盖或删除当前使用的演员照片.*其它导出位置/)
  assert.match(actorManagementError('actor_name_conflict'), /姓名.*相同/)
  assert.match(actorManagementError('actor_export_partial:3:actor_export_unsafe'), /已完成 3 位演员导出，其余未完成/)
})

test('actual save API serializes the full payload and retires photo aliases before refresh', async () => {
  const { api, calls, events, pending } = actorManagementCommands()
  const input = actorInput({ name: 'Mira', aliases: 'Mi', ...profile }, ['film-one'], 'saved', 'Synthetic/photo.png', false)
  const save = api.saveManagedActor(input)
  assert.equal(calls[0].command, 'save_managed_actor')
  assert.equal(calls[0].payload.input, input)
  await assert.rejects(api.saveManagedActor(input), reason => reason === 'actor_save_pending')
  assert.equal(calls.length, 1)
  pending[0].resolve('saved')
  assert.equal(await save, 'saved')
  assert.deepEqual(events.map(event => event.type), ['saved', 'invalidate', 'refresh'])
  assert.deepEqual(Array.from(events[1].payload), ['actor:saved'])
  const retry = api.saveManagedActor({ ...input, photoPath: null })
  pending[1].resolve('saved')
  await retry
  assert.equal(events.filter(event => event.type === 'invalidate').length, 1, 'profile-only saves keep successful photos')
})

test('failed save releases the writer without publishing a false success or refresh', async () => {
  const { api, events, pending } = actorManagementCommands()
  const input = actorInput({ name: 'Mira', aliases: '', ...profile }, [], 'saved', null, true)
  const failed = api.saveManagedActor(input)
  pending[0].reject('protection_failed')
  await assert.rejects(failed, reason => reason === 'protection_failed')
  assert.deepEqual(events, [])
  const retry = api.saveManagedActor(input)
  pending[1].resolve('saved')
  await retry
  assert.deepEqual(events.map(event => event.type), ['saved', 'invalidate', 'refresh'])
})
