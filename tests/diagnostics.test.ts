import assert from 'node:assert/strict'
import test from 'node:test'
import { createDiagnosticInvoke, exceptionEvent, taskCounts } from '../src/data/diagnosticEvents.ts'

test('each repeated task retains its own failure record and operation id', async () => {
  const events: any[] = []
  const invoke = createDiagnosticInvoke(async () => { throw 'translation_auth' }, event => events.push(event))
  for (let n = 0; n < 12; n++) await assert.rejects(invoke('start_title_translation'))
  assert.equal(events.filter(e => e.event === 'failed').length, 12)
  assert.equal(new Set(events.filter(e => e.event === 'failed').map(e => e.operationId)).size, 12)
})

test('IPC logging never reads credential arguments/responses and preserves return values', async () => {
  const events: any[] = []
  let received: any
  const invoke = createDiagnosticInvoke(async (_command, args) => { received = args; return 'SECRET-KEY' }, event => { events.push(event) }, () => 10)
  const key = await invoke('load_translation_key', { provider: 'deepl' })
  assert.equal(key, 'SECRET-KEY')
  assert.equal(events.length, 0)
  await invoke('confirm_actor_scan_add', { root: 'D:\\Private', key: 'SECRET-KEY' })
  assert.equal(received.key, 'SECRET-KEY')
  assert.match(received.diagnosticOperationId, /^op-[a-z0-9-]+$/)
  assert.deepEqual(events.map(e => e.event), ['start', 'complete'])
  assert.equal(events[0].operationId, events[1].operationId)
  assert(!JSON.stringify(events).includes('SECRET-KEY'))
  assert(!JSON.stringify(events).includes('Private'))
})

test('logger throwing or rejecting never replaces the business result or error', async () => {
  const businessError = new Error('SECRET-KEY D:\\Private')
  const invoke = createDiagnosticInvoke(async () => { throw businessError }, () => { throw new Error('disk full') })
  await assert.rejects(invoke('confirm_library_source'), error => error === businessError)
  const good = createDiagnosticInvoke(async () => 7, () => Promise.reject(new Error('disk full')))
  assert.equal(await good('confirm_library_source'), 7)
})

test('summary extraction failure never replaces a successful business result', async () => {
  const result = { get imported(): number { throw new Error('unreadable diagnostic field') } }
  const invoke = createDiagnosticInvoke(async () => result, () => {})
  assert.equal(await invoke('confirm_actor_scan_add'), result)
})

test('only fixed numeric result summaries are extracted', () => {
  const result = { imported: 2, updated: 3, failed: 1, root: 'SECRET-KEY', relationships: { available: 7, unmatched: ['SECRET-KEY'] } }
  assert.deepEqual(taskCounts('confirm_actor_scan_add', result), { imported: 2, updated: 3, failed: 1, available: 7 })
  assert.deepEqual(taskCounts('load_translation_key', result), {})
})

test('exceptions retain type and source position without arbitrary messages or stack text', () => {
  const error = new TypeError('SECRET-KEY D:\\Private')
  error.stack = 'TypeError: SECRET-KEY\n at edit (http://tauri.localhost/assets/index-AbC123.js:42:7)\n at D:\\Private'
  const event = exceptionEvent('unhandled_rejection', error)
  assert.equal(event.errorCode, 'javascript_type_error')
  assert.equal(event.line, 42)
  assert.equal(event.column, 7)
  assert(!JSON.stringify(event).includes('SECRET-KEY'))
  assert(!JSON.stringify(event).includes('Private'))
})
