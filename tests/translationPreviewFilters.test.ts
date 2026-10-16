import test from 'node:test'
import assert from 'node:assert/strict'
import * as translation from '../src/data/titleTranslation.ts'

const entries = [
  {film_id:'a',original_title:'Fictional Ocean',title:'海洋',body:'Ocean',status:'pending' as const,sources:[{id:'one',name:'片库一'}]},
  {film_id:'b',original_title:'Fictional Forest',body:'Forest',status:'pending' as const,sources:[{id:'two',name:'片库二'}]},
  {film_id:'c',original_title:'Fictional Ocean II',body:'Ocean II',status:'pending' as const,sources:[{id:'two',name:'片库二'}]},
]
type PreviewEntries = typeof entries
test('translation preview filters instantly by title and source, empty query restores entries', () => {
  const helpers = translation as unknown as {filterTranslationEntries:(items:PreviewEntries,query:string,source:string)=>PreviewEntries}
  assert.equal(typeof helpers.filterTranslationEntries, 'function')
  assert.deepEqual(helpers.filterTranslationEntries(entries,' ocean ','two').map(e=>e.film_id), ['c'])
  assert.deepEqual(helpers.filterTranslationEntries(entries,'海洋','all').map(e=>e.film_id), ['a'])
  assert.deepEqual(helpers.filterTranslationEntries(entries,'','all'), entries)
})
test('visible select-all preserves hidden selection and toggles only the visible IDs', () => {
  const helpers = translation as unknown as {toggleVisibleTranslationIds:(selected:string[],visible:string[])=>string[]}
  assert.equal(typeof helpers.toggleVisibleTranslationIds, 'function')
  assert.deepEqual(helpers.toggleVisibleTranslationIds(['a'], ['b','c']), ['a','b','c'])
  assert.deepEqual(helpers.toggleVisibleTranslationIds(['a','b','c'], ['b','c']), ['a'])
  assert.deepEqual(helpers.toggleVisibleTranslationIds(['a'], []), ['a'])
})

test('status filters intersect source and search, clearing search stays in the status range', () => {
  const items: translation.TranslationEntry[] = [
    ...entries,
    {film_id:'translated',original_title:'Ocean III',title:'海洋三',body:'Ocean III',status:'automatic',sources:[{id:'two',name:'片库二'}]},
    {film_id:'stale',original_title:'Forest II',title:'森林二',body:'Forest II',status:'stale',sources:[{id:'one',name:'片库一'}]},
    {film_id:'uncertain',original_title:'海洋漢字',body:'海洋漢字',status:'language_uncertain',sources:[{id:'two',name:'片库二'}]},
    {film_id:'reuse',original_title:'Ocean IV',body:'Ocean IV',status:'local_memory'},
  ]
  const filter = translation.filterTranslationEntries as (items:translation.TranslationEntry[],query:string,source:string,status?:string)=>translation.TranslationEntry[]
  assert.deepEqual(filter(items,'','all','translated').map(e=>e.film_id), ['translated','stale'])
  assert.deepEqual(filter(items,'海洋','two','translated').map(e=>e.film_id), ['translated'])
  assert.deepEqual(filter(items,'海洋','two','uncertain').map(e=>e.film_id), ['uncertain'])
  assert.deepEqual(filter(items,'','two','uncertain').map(e=>e.film_id), ['uncertain'])
  assert.deepEqual(filter(items,'','all','all'), items)
})


test('edited titles remain visible under translated filtering but cannot be selected for API overwrite', () => {
  const items: translation.TranslationEntry[] = [{film_id:'edited',original_title:'Fictional Original',title:'编辑片名',body:'Fictional Original',status:'manual',sources:[{id:'two',name:'片库二'}]}]
  assert.deepEqual(translation.filterTranslationEntries(items,'编辑','two','translated'), items)
  assert.deepEqual(translation.defaultTranslationIds(items,true), [])
  assert.deepEqual(translation.defaultTranslationIds(items,false), [])
  assert.equal(translation.selectedTranslationCounts(items,['edited'],true).apiRequests,0)
})
