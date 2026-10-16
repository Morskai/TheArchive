import assert from 'node:assert/strict'
import test from 'node:test'
import { createFavoriteQueue, createFavoriteReadGuard } from '../src/data/filmFavoriteState.ts'
const tick = () => new Promise<void>(resolve => setImmediate(resolve))
function deferred() { let resolve!: () => void; let reject!: (error: Error) => void; const promise = new Promise<void>((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
test('favorite accepts reversal immediately and serializes the final choice', async () => {
 const requests: boolean[] = []; const gates = [deferred(), deferred()]
 const queue = createFavoriteQueue(false, async value => { requests.push(value); await gates[requests.length - 1].promise })
 queue.toggle(); assert.equal(queue.snapshot().favorite, true)
 queue.toggle(); assert.equal(queue.snapshot().favorite, false); assert.deepEqual(requests, [true])
 gates[0].resolve(); await tick(); assert.deepEqual(requests, [true, false]); assert.equal(queue.snapshot().favorite, false)
 gates[1].resolve(); await tick(); assert.deepEqual(queue.snapshot(), { favorite: false, saving: false, error: false })
})
test('intermediate clicks collapse and old saved props cannot overwrite a pending choice', async () => {
 const gate = deferred(); const writes: boolean[] = []
 const queue = createFavoriteQueue(false, async value => { writes.push(value); await gate.promise })
 queue.toggle(); queue.toggle(); queue.toggle(); queue.syncSaved(false)
 assert.equal(queue.snapshot().favorite, true); gate.resolve(); await tick(); assert.deepEqual(writes, [true]); assert.equal(queue.snapshot().saving, false)
 queue.syncSaved(false); assert.equal(queue.snapshot().favorite, false)
})
test('write failure restores the last committed value and permits a retry', async () => {
 const gates = [deferred(), deferred(), deferred()]; let count = 0
 const queue = createFavoriteQueue(false, async () => gates[count++].promise)
 queue.toggle(); queue.toggle(); gates[0].resolve(); await tick(); gates[1].reject(new Error('failed')); await tick()
 assert.deepEqual(queue.snapshot(), { favorite: true, saving: false, error: true })
 queue.toggle(); gates[2].resolve(); await tick(); assert.deepEqual(queue.snapshot(), { favorite: false, saving: false, error: false })
})
test('pending queue survives unsubscribe and a reopened subscriber sees the latest choice', async () => {
 const gate = deferred(); const queue = createFavoriteQueue(false, async () => gate.promise); let notices = 0
 const unsubscribe = queue.subscribe(() => notices++); queue.toggle(); unsubscribe()
 queue.toggle(); const reopen = queue.subscribe(() => notices++); assert.equal(queue.snapshot().favorite, false)
 gate.resolve(); await tick(); assert.equal(queue.snapshot().saving, false); assert.ok(notices > 1); reopen()
})
test('late catalogue reads retain favorite changes committed after the read started', async () => {
 const guard = createFavoriteReadGuard(); let resolve!: (rows: { id: string; favorite: boolean }[]) => void
 const result = guard.read(() => new Promise<{ id: string; favorite: boolean }[]>(yes => { resolve = yes }), (rows, patch) => patch(rows))
 guard.commit('a', true); resolve([{ id: 'a', favorite: false }, { id: 'b', favorite: false }])
 assert.deepEqual(await result, [{ id: 'a', favorite: true }, { id: 'b', favorite: false }])
 const restored = await guard.read(async () => [{ id: 'a', favorite: false }], (rows, patch) => patch(rows))
 assert.equal(restored[0].favorite, false)
})
test('overlapping and rejected reads release patches without retaining an old favorite', async () => {
 const guard = createFavoriteReadGuard(); const gates = [deferred(), deferred()]
 const reads = gates.map(gate => guard.read(async () => { await gate.promise; return [{ id: 'a', favorite: false }] }, (rows, patch) => patch(rows)))
 const rejected = reads[1].catch(() => null); guard.commit('a', true); gates[1].reject(new Error('failed')); assert.equal(await rejected, null)
 gates[0].resolve(); assert.equal((await reads[0])[0].favorite, true)
 assert.equal((await guard.read(async () => [{ id: 'a', favorite: false }], (rows, patch) => patch(rows)))[0].favorite, false)
})
