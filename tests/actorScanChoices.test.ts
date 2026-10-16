import assert from 'node:assert/strict'
import test from 'node:test'
import * as state from '../src/data/actorManagementState.ts'

const item = (key: string, nameAliasSignature: string, status: 'new' | 'conflict' = 'new') => ({ key, directory: `Synthetic/${key}`, name: 'Mira', aliases: ['Mi'], nameAliasSignature, status, reason: null, candidateIds: status === 'conflict' ? ['saved'] : [], canCreate: status !== 'conflict' })

test('partial identity requires explicit choice even when exactly one actor matches', () => {
  const items = [item('a', 'identity', 'conflict'), item('b', 'other')]
  assert.equal(state.pendingActorScanChoice(items, [])?.key, 'a')
  assert.equal(state.pendingActorScanChoice(items, [{ key: 'a', actorId: 'saved', createNew: false }]), undefined)
})

test('two directories updating one actor cannot silently choose last photo', () => {
  const items = [item('a', 'one', 'conflict'), item('b', 'two', 'conflict')]
  const choices = [{ key: 'a', actorId: 'saved', createNew: false }, { key: 'b', actorId: 'saved', createNew: false }]
  assert.deepEqual(state.actorScanBatchConflicts(items, choices).map(group => group.map(row => row.key)), [['a', 'b']])
  const selected = state.chooseActorScanBatch(items, choices, ['a', 'b'], 'b')
  assert.deepEqual(selected.find(row => row.key === 'a'), { key: 'a', actorId: null, createNew: false })
  assert.equal(selected.find(row => row.key === 'b')?.actorId, 'saved')
  assert.deepEqual(state.actorScanBatchConflicts(items, selected), [])
})

test('new actor duplicate batch groups unordered identities and supports one or skip all', () => {
  const items = [item('a', 'same'), item('b', 'same'), item('c', 'other')]
  assert.deepEqual(state.actorScanBatchConflicts(items, []).map(group => group.map(row => row.key)), [['a', 'b']])
  const choices = state.chooseActorScanBatch(items, [], ['a', 'b'], 'a')
  assert.deepEqual(state.actorScanBatchConflicts(items, choices), [])
  assert.equal(choices.find(row => row.key === 'a')?.createNew, true)
  assert.equal(choices.find(row => row.key === 'b')?.createNew, false)
  const skipped = state.chooseActorScanBatch(items, [], ['a', 'b'], null)
  assert.deepEqual(state.actorScanBatchConflicts(items, skipped), [])
})

test('failed directories neither prompt candidate confirmation nor join batch conflicts', () => {
  const items = [{ ...item('a', 'same'), status: 'failed' as const }, item('b', 'same')]
  assert.equal(state.pendingActorScanChoice(items, []), undefined)
  assert.deepEqual(state.actorScanBatchConflicts(items, []), [])
})


test('exact automatic updates participate in repeated target protection and choose-one selection', () => {
  const items = [item('a','same'), item('b','same')].map(row=>({...row,status:'update' as const,canCreate:false,candidateIds:['saved'],defaultActorId:'saved'}))
  assert.equal(state.pendingActorScanChoice(items,[]),undefined)
  assert.deepEqual(state.actorScanBatchConflicts(items,[]).map(group=>group.map(row=>row.key)),[['a','b']])
  const choices=state.chooseActorScanBatch(items,[],['a','b'],'b')
  assert.deepEqual(choices,[{key:'a',actorId:null,createNew:false},{key:'b',actorId:'saved',createNew:false}])
  assert.deepEqual(state.actorScanBatchConflicts(items,choices),[])
})
