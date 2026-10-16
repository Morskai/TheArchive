import assert from 'node:assert/strict'
import test from 'node:test'
import * as state from '../src/data/actorManagementState.ts'

const relation = (filmId: string | null, existingActorIds: string[] = []) => ({ folderName: 'Folder', videoFiles: ['one.mp4', 'two.mkv'], filmId, existingActorIds, reason: filmId ? null : '未找到完全匹配的影片' })
const item = (key: string, filmography = [relation('one', ['saved']), relation('two'), relation(null)]) => ({ key, directory: 'Synthetic', name: key, aliases: [], candidateIds: ['saved'], status: 'conflict' as const, reason: null, canCreate: false, nameAliasSignature: key, filmography })

test('relationship preview follows explicit actor choice and counts actor-film pairs', () => {
  const items = [item('A'), { ...item('B'), status: 'new' as const, candidateIds: [] }]
  assert.deepEqual(state.actorScanRelationships(items, [{ key: 'A', actorId: 'saved', createNew: false }]), {
    available: 3, existing: 1, notFound: 2,
    unmatched: [{ actorName: 'A', folderName: 'Folder', videoFiles: ['one.mp4', 'two.mkv'], reason: '未找到完全匹配的影片' }, { actorName: 'B', folderName: 'Folder', videoFiles: ['one.mp4', 'two.mkv'], reason: '未找到完全匹配的影片' }],
  })
  assert.equal(state.actorScanRelationships([item('A')], [{ key: 'A', actorId: null, createNew: true }]).available, 2)
})

test('skipped failed unresolved and legacy entries do not contribute relationships', () => {
  const skipped = item('skip')
  const failed = { ...item('failed'), status: 'failed' as const }
  const legacy = { ...item('legacy'), status: 'new' as const, filmography: undefined }
  assert.deepEqual(state.actorScanRelationships([skipped, failed, legacy, item('unresolved')], [{ key: 'skip', actorId: null, createNew: false }]), { available: 0, existing: 0, notFound: 0, unmatched: [] })
})


test('automatic exact updates use their own actor id for existing relationship counts', () => {
  const incoming={...item('A'),status:'update' as const,defaultActorId:'saved'}
  const counts=state.actorScanRelationships([incoming],[])
  assert.equal(counts.existing,1);assert.equal(counts.available,1);assert.equal(counts.notFound,1)
  assert.deepEqual(state.actorScanRelationships([incoming],[{key:'A',actorId:null,createNew:false}]),{available:0,existing:0,notFound:0,unmatched:[]})
})
