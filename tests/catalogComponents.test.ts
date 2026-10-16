import assert from 'node:assert/strict'
import test from 'node:test'
import { catalogComponents } from './helpers/catalogComponents.ts'
import type { RealFilm } from '../src/data/realFilms'

const film: RealFilm = { id: 'film-one', title: '虚构影片', manual_code: null, needs_organization: false, rating: null, favorite: false, tags: [], videos: [{ id: 'video-one', observed_path: 'Z:/FICTIONAL_ONLY/film/video.mp4', source_id: 'fictional', source_name: '虚构片库' }] }
const snapshot = { actors: [], films: [film], tags: ['新标签'] }
const count = (commands: string[], command: string) => commands.filter(value => value === command).length

test('App and the actual film-library effects share one full catalog request per refresh', async () => {
  const components = catalogComponents(snapshot)
  try {
    await components.flush()
    assert.equal(count(components.commands, 'load_actor_catalog'), 1)
    assert.equal(count(components.commands, 'load_real_films'), 0)
    assert.equal(count(components.commands, 'list_real_tags'), 0)
    components.commands.length = 0
    components.change()
    await components.flush()
    assert.equal(count(components.commands, 'load_actor_catalog'), 1)
    assert.equal(count(components.commands, 'load_real_films'), 0)
    assert.equal(count(components.commands, 'list_real_tags'), 0)
    assert.deepEqual(Array.from(components.props().films, (film: RealFilm) => film.id), ['film-one'])
    components.commands.length = 0
    components.defer()
    for (let index = 0; index < 10; index++) components.change()
    await components.flush()
    assert.equal(count(components.commands, 'load_actor_catalog'), 10)
    assert.equal(count(components.commands, 'load_real_films'), 0)
    assert.equal(count(components.commands, 'list_real_tags'), 0)
  } finally { components.dispose() }
})

test('shared catalog failure reaches the film page and a fresh read can recover', async () => {
  const components = catalogComponents(snapshot)
  try {
    await components.flush()
    components.defer()
    components.change()
    await components.flush()
    components.pending[0].reject('protection_failed')
    await components.flush()
    assert.equal(components.props().error, 'protection_failed')
    assert.ok(components.text().includes('建立应用资料保护备份失败'))
    assert.equal(components.props().films[0].id, 'film-one')
    components.change()
    await components.flush()
    components.pending[1].resolve(structuredClone(snapshot))
    await components.flush()
    assert.equal(components.props().error, '')
    components.dispose()
    components.commands.length = 0
    components.change()
    await components.flush()
    assert.equal(components.commands.length, 0, 'unmounted catalog listeners must stop requesting data')
  } finally { components.dispose() }
})

test('film tag writes share the snapshot immediately and a newer refresh wins over a late read', async () => {
  const components = catalogComponents(snapshot)
  try {
    await components.flush()
    components.defer()
    components.change()
    await components.flush()
    assert.equal(components.pending.length, 1)
    await components.drawer().onTags('film-one', ['新标签'])
    components.manual('film-one', { rating: 4, preferred_video_id: 'video-one' })
    components.favorite('film-one', true)
    await components.flush()
    assert.equal(components.pending.length, 2, 'a successful edit must start a new catalog generation')
    assert.ok(components.props().films, 'the film library must render the shared raw film snapshot')
    assert.deepEqual(Array.from(components.props().films[0].tags), ['新标签'])
    const fresh = structuredClone(snapshot)
    fresh.films[0].tags = ['新标签']
    components.pending[1].resolve(fresh)
    await components.flush()
    components.pending[0].resolve(structuredClone(snapshot))
    await components.flush()
    const saved = components.props().films[0]
    assert.deepEqual(Array.from(saved.tags), ['新标签'])
    assert.equal(saved.rating, 4)
    assert.equal(saved.preferred_video_id, 'video-one')
    assert.equal(saved.favorite, true)
  } finally { components.dispose() }
})

test('actor-entry film drawer keeps consecutive tag additions while the catalog refresh is pending', async () => {
  const components = catalogComponents({ ...snapshot, tags: ['新标签', '第二标签'] })
  try {
    await components.flush()
    components.openAppFilm('film-one')
    await components.flush()
    components.defer()
    await components.detailTag('新标签').onClick()
    await components.flush()
    assert.equal(components.detailTag('第二标签').disabled, false)
    await components.detailTag('第二标签').onClick()
    await components.flush()
    assert.ok(components.pending.length >= 2, 'catalog replies must remain pending during both edits')
    const writes = components.calls.filter(call => call.command === 'set_real_film_tags')
    assert.deepEqual(Array.from(writes[1].args.names), ['新标签', '第二标签'])
    assert.equal(components.detailTag('新标签')['aria-pressed'], true)
    assert.equal(components.detailTag('第二标签')['aria-pressed'], true)
  } finally { components.dispose() }
})

test('history film drawer keeps consecutive tag additions while the history refresh is pending', async () => {
  const components = catalogComponents({ ...snapshot, tags: ['新标签', '第二标签'] })
  try {
    await components.flush()
    components.mountHistory()
    await components.flush()
    components.openHistoryFilm('film-one')
    await components.flush()
    components.defer()
    components.deferHistory()
    await components.detailTag('新标签').onClick()
    await components.flush()
    assert.equal(components.detailTag('第二标签').disabled, false)
    await components.detailTag('第二标签').onClick()
    await components.flush()
    assert.equal(components.pendingHistory.length, 2, 'history replies must remain pending during both edits')
    const writes = components.calls.filter(call => call.command === 'set_real_film_tags')
    assert.deepEqual(Array.from(writes[1].args.names), ['新标签', '第二标签'])
    assert.equal(components.detailTag('新标签')['aria-pressed'], true)
    assert.equal(components.detailTag('第二标签')['aria-pressed'], true)
  } finally { components.dispose() }
})

test('tag additions survive switching from the actor-entry drawer to the history drawer before refresh completes', async () => {
  const components = catalogComponents({ ...snapshot, tags: ['新标签', '第二标签'] })
  try {
    await components.flush()
    components.mountHistory()
    await components.flush()
    components.openAppFilm('film-one')
    await components.flush()
    components.defer()
    components.deferHistory()
    await components.detailTag('新标签').onClick()
    await components.flush()
    components.mountHistory()
    await components.flush()
    components.openHistoryFilm('film-one')
    await components.flush()
    await components.detailTag('第二标签').onClick()
    await components.flush()
    const writes = components.calls.filter(call => call.command === 'set_real_film_tags')
    assert.deepEqual(writes.map(call => Array.from(call.args.names)), [['新标签'], ['新标签', '第二标签']])
    components.openAppFilm('film-one')
    await components.flush()
    assert.equal(components.detailTag('新标签')['aria-pressed'], true)
    assert.equal(components.detailTag('第二标签')['aria-pressed'], true)
  } finally { components.dispose() }
})

test('history reads started before a tag save retain the saved tags when their old response arrives', async () => {
  const components = catalogComponents(snapshot)
  try {
    await components.flush()
    components.openAppFilm('film-one')
    await components.flush()
    components.defer()
    components.deferHistory()
    const history = components.readHistory()
    await components.flush()
    assert.equal(components.pendingHistory.length, 1)
    await components.detailTag('新标签').onClick()
    await components.flush()
    components.pendingHistory[0].resolve({ films: structuredClone(snapshot.films), events: [], film_count: 1, source_count: 1 })
    const rows = await history
    assert.deepEqual(Array.from(rows.films[0].tags), ['新标签'])
  } finally { components.dispose() }
})
