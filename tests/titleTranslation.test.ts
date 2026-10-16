import assert from 'node:assert/strict'
import test from 'node:test'
import { selectedTranslationEntries, selectedTranslationCounts, translationError, type TranslationEntry } from '../src/data/titleTranslation.ts'
const entries: TranslationEntry[] = [
  { film_id:'cn', original_title:'虚构原文', body:'虚构原文', status:'chinese' },
  { film_id:'auto', original_title:'Demo auto', body:'Demo auto', status:'automatic' },
  { film_id:'stale', original_title:'Demo changed', body:'Demo changed', status:'stale' },
  { film_id:'memory', original_title:'Demo memory', body:'Demo memory', status:'local_memory' },
  { film_id:'pending1', original_title:'Demo new', body:'Demo new', status:'pending' },
  { film_id:'pending2', original_title:'Demo new', body:'Demo new', status:'pending' },
]
test('normal selection excludes existing automatic titles and deduplicates API bodies', () => {
  assert.deepEqual(selectedTranslationEntries(entries, entries.map(e=>e.film_id), false).map(e=>e.film_id), ['memory','pending1','pending2'])
  assert.deepEqual(selectedTranslationCounts(entries, entries.map(e=>e.film_id), false), { candidates:3, localReuse:1, apiRequests:1 })
})
test('explicit retranslation selects only automatic results including stale results', () => {
  assert.deepEqual(selectedTranslationEntries(entries, entries.map(e=>e.film_id), true).map(e=>e.film_id), ['auto','stale'])
  assert.deepEqual(selectedTranslationCounts(entries, ['auto'], true), { candidates:1, localReuse:0, apiRequests:1 })
})
test('unknown service error never exposes raw secret-bearing response', () => {
  assert.equal(translationError('private synthetic secret'), '操作未完成，请稍后重试。')
  assert.match(translationError('translation_memory_changed'), /重新检测/)
})

test('uncertain Han language remains eligible only after explicit selection and defaults unchecked', async () => {
  const translation = await import('../src/data/titleTranslation.ts')
  assert.equal(typeof translation.defaultTranslationIds, 'function')
  const uncertain = { film_id:'uncertain', original_title:'虚構教師放課後', body:'虚構教師放課後', status:'language_uncertain' as const }
  const candidates = [...entries, uncertain]
  assert.deepEqual(translation.defaultTranslationIds(candidates, false), ['memory','pending1','pending2'])
  assert.deepEqual(selectedTranslationEntries(candidates, ['uncertain'], false), [uncertain])
  assert.deepEqual(selectedTranslationCounts(candidates, ['uncertain'], false), { candidates:1, localReuse:0, apiRequests:1 })
  assert.deepEqual(translation.defaultTranslationIds(candidates, true), ['auto','stale'])
  assert.deepEqual(selectedTranslationEntries(candidates, ['uncertain'], true), [])
})
