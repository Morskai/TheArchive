import assert from 'node:assert/strict'
import test from 'node:test'
import { CoverCache } from '../src/data/coverCache.ts'

test('portrait remount can read its live session image synchronously without loading or reviving retired images', async () => {
  let reads = 0
  const cache = new CoverCache(async () => { reads++; return new ArrayBuffer(1) }, () => 'blob:portrait', () => {})
  assert.equal(cache.peek('actor:a', true), null)
  assert.equal(reads, 0)
  const lease = cache.acquire('actor:a', true, () => true, () => 0)
  await lease.result
  lease.release()
  assert.equal(cache.peek('actor:a', true), 'blob:portrait')
  assert.equal(cache.peek('actor:a', false), null)
  assert.equal(cache.peek('actor:b', true), null)
  assert.equal(reads, 1)
  cache.invalidateIds(['actor:a'])
  assert.equal(cache.peek('actor:a', true), null)
  assert.equal(reads, 1)
})

test('failed artwork stays stopped across sizes and visits until its explicit successful operation', async () => {
  let reads = 0
  const cache = new CoverCache(async () => { reads++; throw 'cover_unavailable' }, () => 'blob:image', () => {})
  for (const large of [false, true, false, true]) {
    const lease = cache.acquire('actor:a', large, () => true, () => 0)
    await assert.rejects(lease.result, error => error === 'cover_unavailable')
    lease.release()
  }
  assert.equal(reads, 1)
  cache.invalidateIds(['actor:b'])
  const unchanged = cache.acquire('actor:a', true, () => true, () => 0)
  await assert.rejects(unchanged.result)
  unchanged.release()
  assert.equal(reads, 1)
  cache.invalidateIds(['actor:a'])
  const retry = cache.acquire('actor:a', true, () => true, () => 0)
  await assert.rejects(retry.result)
  retry.release()
  assert.equal(reads, 2)
})

test('scan success only releases failures in its supplied scope and retains healthy artwork', async () => {
  const reads: string[] = []
  const cache = new CoverCache(async id => { reads.push(id); if (id !== 'healthy') throw 'cover_unavailable'; return new ArrayBuffer(1) }, () => 'blob:healthy', () => {})
  for (const id of ['a', 'b', 'healthy']) {
    const lease = cache.acquire(id, false, () => true, () => 0)
    try { await lease.result } catch { /* expected missing source */ }
    lease.release()
  }
  assert.deepEqual(cache.refreshMissing(['a', 'healthy']), ['a'])
  for (const id of ['a', 'b', 'healthy']) {
    const lease = cache.acquire(id, true, () => true, () => 0)
    try { await lease.result } catch { /* expected missing source */ }
    lease.release()
  }
  assert.equal(reads.filter(id => id === 'a').length, 2)
  assert.equal(reads.filter(id => id === 'b').length, 1)
  const healthy = cache.acquire('healthy', false, () => true, () => 0)
  assert.equal(await healthy.result, 'blob:healthy')
  healthy.release()
})

test('decoded thumbnail dimensions determine retention and decoded images live until eviction', async () => {
  const disposed: string[] = []
  const cache = new CoverCache(async () => new ArrayBuffer(8), () => 'blob:portrait', () => {}, {
    prepare: async (url: string) => ({ decodedBytes: 320 * 480 * 4, dispose: () => disposed.push(url) }),
  })
  const lease = cache.acquire('portrait', false, () => true, () => 0)
  await lease.result; lease.release()
  assert.equal(cache.retainedBytes, 320 * 480 * 4 + 8)
  assert.deepEqual(disposed, [])
  cache.invalidate()
  assert.deepEqual(disposed, ['blob:portrait'])
  assert.equal(cache.retainedBytes, 0)
})



test('invalidating during decode disposes stale decoded images without publishing a URL', async () => {
  let prepared!: () => void
  const revoked: string[] = [], disposed: string[] = []
  const cache = new CoverCache(async () => new ArrayBuffer(8), () => 'blob:old', url => revoked.push(url), {
    prepare: (url: string) => new Promise(resolve => { prepared = () => resolve({ decodedBytes: 20, dispose: () => disposed.push(url) }) }),
  })
  const lease = cache.acquire('film', false, () => true, () => 0)
  await Promise.resolve(); cache.invalidate(); prepared()
  assert.equal(await lease.result, null)
  lease.release()
  assert.deepEqual(revoked, ['blob:old'])
  assert.deepEqual(disposed, ['blob:old'])
  assert.equal(cache.retainedBytes, 0)
})

test('1236 portrait thumbnails fit actual-size accounting without rereading the first image', async () => {
  let reads = 0
  const cache = new CoverCache(async () => { reads++; return new ArrayBuffer(16_000) }, () => `blob:${reads}`, () => {}, {
    prepare: async () => ({ decodedBytes: 343 * 480 * 4, dispose: () => {} }),
  })
  for (let index = 0; index < 1236; index++) {
    const lease = cache.acquire(`image-${index}`, false, () => true, () => 0)
    await lease.result; lease.release()
  }
  const revisit = cache.acquire('image-0', false, () => true, () => 0)
  assert.equal(await revisit.result, 'blob:1')
  revisit.release()
  assert.equal(reads, 1236)
  assert.ok(cache.retainedBytes < cache.budgetBytes * .9)
})





test('revisiting and concurrent views share a thumbnail without another read', async () => {
  let reads = 0
  const cache = new CoverCache(async () => { reads++; return new ArrayBuffer(8) }, () => `blob:${reads}`, () => {})
  const first = cache.acquire('film', false, () => true, () => 0)
  const second = cache.acquire('film', false, () => true, () => 0)
  assert.equal(await first.result, await second.result)
  first.release(); second.release()
  const again = cache.acquire('film', false, () => true, () => 0)
  assert.equal(await again.result, 'blob:1')
  assert.equal(reads, 1)
  again.release()
})

test('memory is bounded, active images survive eviction, and invalidation rereads', async () => {
  const revoked: string[] = []
  let reads = 0
  const cache = new CoverCache(async () => { reads++; return new ArrayBuffer(8) }, () => `blob:${reads}`, url => revoked.push(url), { maxBytes: 2 * (480 * 480 * 4 + 8) })
  const first = cache.acquire('first', false, () => true, () => 0)
  await first.result
  for (const key of ['second', 'third']) {
    const lease = cache.acquire(key, false, () => true, () => 0)
    await lease.result; lease.release()
  }
  assert.deepEqual(revoked, ['blob:2'])
  cache.invalidate()
  assert.ok(!revoked.includes('blob:1'))
  first.release()
  assert.ok(revoked.includes('blob:1'))
  const fresh = cache.acquire('first', false, () => true, () => 0)
  await fresh.result; fresh.release()
  assert.equal(reads, 4)
})

test('scroll jumps skip abandoned jobs and prioritize visible over nearby cards', async () => {
  const reads: string[] = []
  const finish: Array<() => void> = []
  const cache = new CoverCache(key => { reads.push(key); return new Promise(resolve => finish.push(() => resolve(new ArrayBuffer(8)))) }, () => 'blob:test', () => {})
  const leases = ['running1', 'running2', 'abandoned', 'near', 'visible'].map(key => cache.acquire(key, false, () => key !== 'abandoned', () => key === 'visible' ? 0 : 1))
  assert.deepEqual(reads, ['running1', 'running2'])
  finish.shift()!(); await leases[0].result
  assert.equal(reads[2], 'visible')
  finish.shift()!(); await leases[1].result
  assert.equal(reads[3], 'near')
  finish.shift()!(); finish.shift()!()
  await Promise.all(leases.map(lease => lease.result))
  assert.ok(!reads.includes('abandoned'))
  leases.forEach(lease => lease.release())
})

test('session images survive long elapsed time and reread only after explicit invalidation', async () => {
  let now = 0, available = true
  let reads = 0
  const realNow = Date.now
  Date.now = () => now
  const revoked: string[] = []
  const cache = new CoverCache(async () => { reads++; if (!available) throw 'cover_unavailable'; return new ArrayBuffer(8) }, () => 'blob:old', url => revoked.push(url))
  try {
  const first = cache.acquire('film', false, () => true, () => 0)
  await first.result; first.release()
  now = 3_600_000; available = false
  const restored = cache.acquire('film', false, () => true, () => 0)
  assert.equal(await restored.result, 'blob:old')
  restored.release()
  assert.equal(reads, 1)
  cache.invalidate()
  const next = cache.acquire('film', false, () => true, () => 0)
  await assert.rejects(next.result, reason => reason === 'cover_unavailable')
  assert.deepEqual(revoked, ['blob:old'])
  next.release()
  const failedAgain = cache.acquire('film', false, () => true, () => 0)
  await assert.rejects(failedAgain.result, reason => reason === 'cover_unavailable')
  failedAgain.release()
  assert.equal(reads, 2)
  } finally { Date.now = realNow }
})

test('default session budget retains over 96 visited images and accounts for decoded memory', async () => {
  let reads = 0
  const cache = new CoverCache(async () => { reads++; return new ArrayBuffer(8) }, () => `blob:${reads}`, () => {})
  for (let i = 0; i < 120; i++) {
    const lease = cache.acquire(`film-${i}`, false, () => true, () => 0)
    await lease.result; lease.release()
  }
  const first = cache.acquire('film-0', false, () => true, () => 0)
  assert.equal(await first.result, 'blob:1')
  first.release()
  assert.equal(reads, 120)
  assert.equal(cache.retainedBytes, 120 * (480 * 480 * 4 + 8))
  assert.equal(cache.budgetBytes, Number.POSITIVE_INFINITY)
})

test('budget eviction notifies offscreen views and large thumbnails reserve larger decoded cost', async () => {
  let reads = 0
  const disposed: string[] = []
  const largeCost = 1200 * 1200 * 4 + 8
  const cache = new CoverCache(async () => { reads++; return new ArrayBuffer(8) }, () => `blob:${reads}`, () => {}, { maxBytes: largeCost })
  const unsubscribe = cache.subscribe('small', false, url => disposed.push(url))
  const large = cache.acquire('large', true, () => true, () => 0)
  await large.result
  assert.equal(cache.retainedBytes, largeCost)
  const small = cache.acquire('small', false, () => true, () => 0)
  await small.result; small.release()
  assert.deepEqual(disposed, ['blob:2'])
  assert.equal(cache.retainedBytes, largeCost)
  large.release(); cache.invalidate()
  assert.equal(cache.retainedBytes, 0)
  unsubscribe()
})

test('scan invalidation during loading cannot repopulate the new session with stale bytes', async () => {
  const finish: Array<() => void> = []
  let created = 0
  const cache = new CoverCache(() => new Promise(resolve => finish.push(() => resolve(new ArrayBuffer(8)))), () => `blob:${++created}`, () => {})
  const old = cache.acquire('film', false, () => true, () => 0)
  cache.invalidate()
  const fresh = cache.acquire('film', false, () => true, () => 0)
  finish.shift()!(); assert.equal(await old.result, null)
  finish.shift()!(); assert.equal(await fresh.result, 'blob:1')
  old.release(); fresh.release()
  assert.equal(created, 1)
  assert.equal(cache.retainedBytes, 480 * 480 * 4 + 8)
})

test('retired images stay accounted until all views release without clearing a newer URL', async () => {
  const disposed: string[] = []
  let created = 0
  const cache = new CoverCache(async () => new ArrayBuffer(8), () => `blob:${++created}`, () => {})
  const unsubscribe = cache.subscribe('film', false, url => disposed.push(url))
  const old = cache.acquire('film', false, () => true, () => 0)
  await old.result
  cache.invalidate()
  const fresh = cache.acquire('film', false, () => true, () => 0)
  await fresh.result
  assert.equal(cache.retainedBytes, 2 * (480 * 480 * 4 + 8))
  old.release()
  assert.deepEqual(disposed, ['blob:1'])
  assert.equal(cache.retainedBytes, 480 * 480 * 4 + 8)
  const shared = cache.acquire('film', false, () => true, () => 0)
  assert.equal(await shared.result, 'blob:2')
  shared.release(); fresh.release(); cache.invalidate()
  assert.equal(cache.retainedBytes, 0)
  unsubscribe()
})


test('scan refresh retries missing artwork without revoking successful cached URLs', async () => {
  const reads: string[] = [], revoked: string[] = []
  let available = false
  const cache = new CoverCache(async id => {
    reads.push(id)
    if (id === 'missing' && !available) throw new Error('missing')
    return new ArrayBuffer(8)
  }, () => `blob:${reads.length}`, url => revoked.push(url))
  const good = cache.acquire('good', false, () => true, () => 0)
  const url = await good.result
  good.release()
  const missing = cache.acquire('missing', false, () => true, () => 0)
  await assert.rejects(missing.result)
  missing.release()
  available = true
  assert.deepEqual(cache.refreshMissing(['missing', 'good']), ['missing'])
  const retained = cache.acquire('good', false, () => true, () => 0)
  assert.equal(await retained.result, url)
  retained.release()
  const retry = cache.acquire('missing', false, () => true, () => 0)
  assert.ok(await retry.result)
  retry.release()
  assert.deepEqual(reads, ['good', 'missing', 'missing'])
  assert.deepEqual(revoked, [])
})
