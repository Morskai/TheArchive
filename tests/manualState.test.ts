import assert from 'node:assert/strict'
import test from 'node:test'
import { createSavedValueQueue, createManualReadGuard } from '../src/data/manualState.ts'
const deferred = <T>() => { let resolve!: (value: T) => void; let reject!: () => void; const promise = new Promise<T>((a,b) => { resolve=a; reject=b }); return { promise, resolve, reject } }
const tick = () => new Promise(resolve => setImmediate(resolve))
test('rapid rating requests immediately reflect the newest choice and save serially', async () => {
  const writes: { value: number | null; done: ReturnType<typeof deferred<void>> }[] = []
  const q = createSavedValueQueue<number | null>(null, value => { const done=deferred<void>(); writes.push({ value, done }); return done.promise })
  q.set(5); q.set(2); q.set(4)
  assert.equal(q.snapshot().value,4); assert.equal(writes.length,1)
  q.syncSaved(null); assert.equal(q.snapshot().value,4)
  writes[0].done.resolve(); await tick(); assert.equal(writes[1].value,4)
  writes[1].done.resolve(); await tick(); assert.deepEqual(q.snapshot(),{ value:4,saving:false,error:false })
})
test('preferred cancellation receives the last successfully stored version', async () => {
  const writes: [string | null,string | null][]=[]; const gate=deferred<void>()
  const q=createSavedValueQueue<string | null>(null,async (value,committed)=>{ writes.push([value,committed]); if(writes.length===1) await gate.promise })
  q.set('version-b'); q.set(null); assert.equal(q.snapshot().value,null)
  gate.resolve(); await tick(); assert.deepEqual(writes,[['version-b',null],[null,'version-b']])
})
test('failed write restores the latest committed value and can retry', async () => {
  let fail=false; const q=createSavedValueQueue(false,async()=>{ if(fail) throw Error('synthetic') })
  q.set(true); await tick(); fail=true; q.set(false); await tick()
  assert.deepEqual(q.snapshot(),{value:true,saving:false,error:true}); fail=false; q.set(false); await tick(); assert.equal(q.snapshot().value,false)
})
test('overlapping stale reads preserve independent rating and preferred patches then release them', async () => {
  const guard=createManualReadGuard<{rating:number|null;preferred_video_id:string|null}>()
  const a=deferred<{id:string;rating:number|null;preferred_video_id:string|null}[]>(); const b=deferred<typeof a.promise extends Promise<infer T>?T:never>()
  const first=guard.read(()=>a.promise,(rows,patch)=>patch(rows)); const second=guard.read(()=>b.promise,(rows,patch)=>patch(rows))
  guard.commit('film',{rating:4}); guard.commit('film',{preferred_video_id:'b'})
  const old=[{id:'film',rating:null,preferred_video_id:null}]; a.resolve(old); b.resolve(old)
  assert.deepEqual(await first,[{id:'film',rating:4,preferred_video_id:'b'}]); assert.deepEqual(await second,await first)
  assert.deepEqual(await guard.read(async()=>old,(rows,patch)=>patch(rows)),old)
})
