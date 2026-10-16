import assert from 'node:assert/strict'
import test from 'node:test'
import { formatMessage, message } from '../src/i18n/core.ts'
import { createLanguageStore } from '../src/i18n/store.ts'

test('messages keep literal user content and repeated placeholders intact', () => {
  const catalogs = { en: { '已删除 {0} 部影片。': 'Deleted {0} films.', '{0}／{0}：{1}': '{1}: {0}/{0}' }, 'zh-CN': {} }
  const userTitle = '设置 {0} $& <script>保存</script>'
  assert.equal(formatMessage(catalogs, 'en', message('{0}／{0}：{1}', [4, userTitle])), `${userTitle}: 4/4`)
  assert.equal(formatMessage(catalogs, 'en', message('已删除 {0} 部影片。', [2])), 'Deleted 2 films.')
  assert.equal(formatMessage(catalogs, 'zh-CN', message('已删除 {0} 部影片。', [2])), '已删除 2 部影片。')
})

test('pending software notices follow a language switch while nested values remain literal', () => {
  const notice = message('保存失败：{0}', [message('请重试。')])
  const catalogs = { en: { '保存失败：{0}': 'Could not save: {0}', '请重试。': 'Please try again.' }, ja: { '保存失败：{0}': '保存できませんでした。{0}', '请重试。': 'もう一度お試しください。' } }
  assert.equal(formatMessage(catalogs, 'en', notice), 'Could not save: Please try again.')
  assert.equal(formatMessage(catalogs, 'ja', notice), '保存できませんでした。もう一度お試しください。')
})

test('failed reads remain retryable and cannot overwrite saved settings with a default', async () => {
  let fail = true, writes = 0
  const store = createLanguageStore({ load: async () => { if (fail) throw new Error('offline'); return 'ja' }, save: async () => { writes++ } })
  await store.load()
  assert.equal(store.getSnapshot().ready, false)
  assert.equal(store.getSnapshot().error, 'load')
  await store.save('en')
  assert.equal(writes, 0)
  fail = false; await store.load()
  assert.equal(store.getSnapshot().language, 'ja')
  assert.equal(store.getSnapshot().ready, true)
})

test('save failures preserve current language, concurrent writes are ignored, and subscribers are notified', async () => {
  let finish: (() => void) | undefined, reject = false, writes = 0, changes = 0
  const store = createLanguageStore({ load: async () => 'zh-CN', save: async () => { writes++; if (reject) throw new Error('disk'); await new Promise<void>(resolve => { finish = resolve }) } })
  const unsubscribe = store.subscribe(() => changes++)
  await store.load(); reject = true; await store.save('en')
  assert.equal(store.getSnapshot().language, 'zh-CN')
  assert.equal(store.getSnapshot().error, 'save')
  reject = false; const pending = store.save('ja'); await store.save('en')
  assert.equal(writes, 2)
  assert.equal(store.getSnapshot().language, 'zh-CN')
  finish!(); await pending
  assert.equal(store.getSnapshot().language, 'ja')
  assert.ok(changes >= 4)
  const before = changes; unsubscribe(); store.accept('zh-TW'); assert.equal(changes, before)
})

test('a restore/reset event wins over old reads and saves, and invalid language events do nothing', async () => {
  let finishRead: ((value: string) => void) | undefined, finishSave: (() => void) | undefined
  const store = createLanguageStore({ load: () => new Promise(resolve => { finishRead = resolve }), save: () => new Promise(resolve => { finishSave = resolve }) })
  const read = store.load(); store.accept('ja'); finishRead!('en'); await read
  assert.equal(store.getSnapshot().language, 'ja')
  const save = store.save('en'); store.accept('zh-CN'); finishSave!(); await save
  assert.equal(store.getSnapshot().language, 'zh-CN')
  store.accept('unexpected'); assert.equal(store.getSnapshot().language, 'zh-CN')
})
