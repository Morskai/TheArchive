import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'
import { createLibraryLifecycle, dismissResetNotice, maintenanceError, readResetNotice, rememberResetNotice } from '../src/data/maintenance.ts'
import { sourceRefreshNotice } from '../src/data/sources.ts'

test('initialization is shared through strict remounts and later library reads', async () => {
  let calls = 0
  let complete!: (value: unknown) => void
  const api = createLibraryLifecycle(async () => { calls++; return new Promise(resolve => { complete = resolve }) })
  const first = api.initialize()
  const second = api.initialize()
  assert.equal(first, second)
  assert.equal(calls, 1)
  complete({ savedSources: 2 })
  assert.equal((await first).savedSources, 2)
  await api.initialize()
  assert.equal(calls, 1)
})

test('a failed initialization permits a real retry but keeps concurrent requests shared', async () => {
  let calls = 0
  const api = createLibraryLifecycle(async () => {
    if (++calls === 1) throw new Error('offline')
    return { savedSources: 1 }
  })
  await assert.rejects(api.initialize(), /offline/)
  const retry = api.initialize()
  assert.equal(retry, api.initialize())
  await retry
  assert.equal(calls, 2)
})

test('initial-state restoration releases the saved startup result', async () => {
  let calls = 0
  const api = createLibraryLifecycle(async () => ({ savedSources: ++calls === 1 ? 2 : 0 }))
  assert.equal((await api.initialize()).savedSources, 2)
  api.reset()
  assert.equal((await api.initialize()).savedSources, 0)
  assert.equal(calls, 2)
})

test('reset failures distinguish restored old data from pending committed cleanup', () => {
  assert.match(maintenanceError('reset_failed'), /原有资料已保留/)
  assert.match(maintenanceError('reset_cleanup_pending'), /已恢复初始状态/)
  assert.match(maintenanceError('reset_cleanup_pending'), /重启/)
  assert.match(maintenanceError('reset_recovery_required'), /保护资料/)
})

test('reset notifications survive remounts without retaining prior usage data', t => {
  const prior = Object.getOwnPropertyDescriptor(globalThis, 'sessionStorage')
  const values = new Map<string, string>()
  Object.defineProperty(globalThis, 'sessionStorage', { configurable: true, value: {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value) },
    removeItem: (key: string) => { values.delete(key) },
  } })
  t.after(() => {
    if (prior) Object.defineProperty(globalThis, 'sessionStorage', prior)
    else Reflect.deleteProperty(globalThis, 'sessionStorage')
  })
  rememberResetNotice('complete')
  assert.equal(readResetNotice(), 'complete')
  assert.equal(readResetNotice(), 'complete')
  assert.deepEqual([...values.values()], ['complete'])
  dismissResetNotice()
  assert.equal(readResetNotice(), null)
  rememberResetNotice('cleanup_pending')
  assert.equal(readResetNotice(), 'cleanup_pending')
  dismissResetNotice()
  assert.equal(values.size, 0)
})

test('translation credential IPC entry points use the storage recovery barrier', () => {
  const source = readFileSync(new URL('../src-tauri/src/translation_commands.rs', import.meta.url), 'utf8')
  for (const [name, action] of [['load_translation_key', 'read_key'], ['save_translation_key', 'save_key'], ['delete_translation_key', 'delete_key']]) {
    const body = source.split(`pub async fn ${name}(`)[1]?.split('#[tauri::command]')[0] ?? ''
    assert.match(body, /app: tauri::AppHandle/)
    assert.match(body, /with_storage\(app,/)
    assert.match(body, new RegExp(`credentials::${action}`))
    assert.doesNotMatch(body, /spawn_blocking/)
  }
  assert.match(source, /if crate::reset::recovery_pending\(storage\.data_directory\(\)\)/)
})

test('manual scan reports completed observation refresh separately from restoration failure', () => {
  const result = { library_id: 'synthetic', discovered: 1, inserted: 1, missing_count: 0 }
  assert.equal(sourceRefreshNotice(result), '')
  assert.match(sourceRefreshNotice({ ...result, related_retained: 1 }), /相关目录未完成核对，保留原资料/)
  const feedback = sourceRefreshNotice({ ...result, recovery_failed: true })
  assert.match(feedback, /下次扫描会重试/)
  assert.doesNotMatch(feedback, /下次启动/)
  assert.doesNotMatch(feedback, /扫描失败|全部回滚/)
})
