import assert from 'node:assert/strict'
import test from 'node:test'
import * as state from '../src/data/actorManagementState.ts'
import { actorManagementCommands } from './helpers/actorManagementCommands.ts'

test('actor film search intersects exact source ids without combining same-named libraries', () => {
  const films = [
    { id: 'one', title: '夜色', code: 'A-1', library: '主收藏、备用', sources: [{ id: 's1', name: '主收藏' }, { id: 's2', name: '备用' }] },
    { id: 'two', title: '白昼', code: 'B-2', library: '主收藏', sources: [{ id: 's3', name: '主收藏' }] },
  ]
  assert.deepEqual(state.actorEditorSources(films).map(source => source.id), ['s2', 's1', 's3'])
  assert.deepEqual(state.actorEditorFilms(films, 'a-1', 's2').map(film => film.id), ['one'])
  assert.deepEqual(state.actorEditorFilms(films, '', 's3').map(film => film.id), ['two'])
  assert.deepEqual(state.actorEditorFilms(films, '夜', 's3'), [])
})

test('clear personal information preserves identity and aliases', () => {
  assert.deepEqual(state.clearActorPersonalInfo({ name: 'Mira', aliases: 'Mi', birthMonth: '1990-01', height: '168', cup: 'B', bust: '90', waist: '60', hips: '92' }), { name: 'Mira', aliases: 'Mi', ...state.emptyActorProfile })
})

test('scan confirmation retains bound root/token and uses returned changed and successful artwork scopes', async () => {
  const { api, calls, pending, events } = actorManagementCommands()
  const preview = { root: 'Synthetic/actors', token: 'bound', items: [] }
  const confirm = api.confirmActorScanAdd(preview, [{ key: 'one', actorId: 'saved', createNew: false }])
  assert.equal(calls[0].command, 'confirm_actor_scan_add')
  assert.equal(calls[0].payload.root, preview.root)
  assert.equal(calls[0].payload.token, preview.token)
  pending[0].resolve({ imported: 1, updated: 1, skipped: 2, failed: 0, conflicts: 0, artwork_keys: ['actor:changed'], successful_artwork_keys: ['actor:changed', 'actor:unchanged'] })
  await confirm
  assert.deepEqual(events.map(event => event.type), ['invalidate', 'refresh-missing', 'refresh'])
  assert.deepEqual([...events[0].payload], ['actor:changed'])
  assert.deepEqual([...events[1].payload], ['actor:changed', 'actor:unchanged'])
})

test('export preview and confirmation pass the same selected parent', async () => {
  const { api, calls, pending } = actorManagementCommands()
  const preview = api.previewActorExport(['one'], 'Synthetic/chosen')
  assert.equal(calls[0].payload.destination, 'Synthetic/chosen')
  pending[0].resolve({ token: 'bound', directory: 'Synthetic/chosen/ActorExports', items: [] })
  await preview
  const confirm = api.confirmActorExport(['one'], 'Synthetic/chosen', 'bound', true)
  assert.equal(calls[1].payload.destination, 'Synthetic/chosen')
  assert.equal(calls[1].payload.token, 'bound')
  assert.equal(calls[1].payload.overwrite, true)
  pending[1].resolve({ exported: 1, directory: 'Synthetic/chosen/ActorExports' })
  await confirm
})


test('accepted unchanged actor refreshes only missing artwork while skips and failures stay outside the scope', async () => {
  const {api,pending,events}=actorManagementCommands()
  const row=(key:string,id:string)=>({key,name:key,aliases:[],directory:key,status:'update' as const,defaultActorId:id,candidateIds:[id],canCreate:false,nameAliasSignature:key,reason:null})
  const promise=api.confirmActorScanAdd({root:'Synthetic',token:'bound',items:[row('a','saved'),row('b','skipped')]},[{key:'b',actorId:null,createNew:false}])
  pending[0].resolve({imported:0,updated:0,skipped:2,failed:1,conflicts:0,artwork_keys:[],successful_artwork_keys:['actor:saved']});await promise
  assert.deepEqual(events.map(event => event.type), ['invalidate', 'refresh-missing', 'refresh'])
  assert.deepEqual([...events[0].payload],[])
  assert.deepEqual([...events[1].payload],['actor:saved'])
})

test('rejected actor confirmation does not invalidate or refresh cached artwork', async () => {
  const { api, pending, events } = actorManagementCommands()
  const promise = api.confirmActorScanAdd({ root: 'Synthetic', token: 'stale', items: [] })
  pending[0].reject('actor_import_changed')
  await assert.rejects(promise, error => error === 'actor_import_changed')
  assert.deepEqual(events, [])
})

test('skipped or failed actor rows do not infer invalidation from preview candidates', async () => {
  const { api, pending, events } = actorManagementCommands()
  const promise = api.confirmActorScanAdd({ root: 'Synthetic', token: 'bound', items: [] }, [{ key: 'skipped', actorId: 'existing', createNew: false }])
  pending[0].resolve({ imported: 0, updated: 0, skipped: 1, failed: 1, conflicts: 0, artwork_keys: [], successful_artwork_keys: [] })
  await promise
  assert.deepEqual([...events[0].payload], [])
  assert.deepEqual([...events[1].payload], [])
})
