import assert from 'node:assert/strict'
import test from 'node:test'
import { selectedVisibleIds, visibleRealFilms, type RealFilmFilters } from '../src/data/realFilmFilters.ts'
import type { RealFilm } from '../src/data/realFilms'
import { tagFilmCounts } from '../src/data/tagPresentation.ts'

const first: RealFilm = {
  id: 'film-one', title: '虚构甲', manual_code: null, needs_organization: true,
  rating: 4, favorite: true, tags: ['已看', '精选'],
  videos: [{ id: 'video-one', observed_path: '甲.mp4', source_id: 'source-a', source_name: '虚构来源甲' }],
}
const second: RealFilm = {
  id: 'film-two', title: '虚构乙', manual_code: null, needs_organization: true,
  rating: null, favorite: false, tags: ['已看'],
  videos: [{ id: 'video-two', observed_path: '乙.mp4', source_id: 'source-b', source_name: '虚构来源乙' }],
}
const third: RealFilm = {
  id: 'film-three', title: '虚构丙', manual_code: null, needs_organization: true,
  rating: 2, favorite: true, tags: [],
  videos: [{ id: 'video-three', observed_path: '丙.mp4', source_id: 'source-a', source_name: '虚构来源甲' }],
}
const films = [first, second, third]
const defaults: RealFilmFilters = {
  query: '', source: 'all', rating: 'all', favoritesOnly: false,
  selectedTags: [], sort: 'title', direction: 'asc',
}

test('tag counts follow the intersection of search, source, rating, favorite and tags', () => {
  const counts = (filters: Partial<RealFilmFilters>) => tagFilmCounts(visibleRealFilms(films, { ...defaults, ...filters }))
  assert.equal(counts({})['已看'], 2)
  assert.equal(counts({ source: 'source-a', favoritesOnly: true, rating: '4', query: '虚构甲', selectedTags: ['精选'] })['已看'], 1)
  assert.equal(counts({ source: 'source-b', favoritesOnly: true })['已看'] ?? 0, 0)
  assert.equal(counts({ selectedTags: ['已看', '精选'] })['精选'], 1)
  assert.equal(tagFilmCounts([{ tags: ['重复', '重复', '__proto__'] }])['重复'], 1)
  assert.equal(tagFilmCounts([{ tags: ['__proto__'] }])['__proto__'], 1)
})

test('rating filter matches exact stars and unrestricted includes unrated films', () => {
  assert.equal(visibleRealFilms(films, defaults).length, 3)
  assert.deepEqual(visibleRealFilms(films, { ...defaults, rating: '4' }).map(film => film.id), ['film-one'])
  assert.deepEqual(visibleRealFilms(films, { ...defaults, rating: '2' }).map(film => film.id), ['film-three'])
  for (const rating of ['5', '3', '1'] as const) assert.deepEqual(visibleRealFilms(films, { ...defaults, rating }), [])
})

test('saved favorite, rating, tags and source drive real filters and sorting', () => {
  assert.deepEqual(new Set(visibleRealFilms(films, { ...defaults, favoritesOnly: true }).map(film => film.id)), new Set(['film-one', 'film-three']))
  assert.deepEqual(visibleRealFilms(films, { ...defaults, selectedTags: ['已看', '精选'] }).map(film => film.id), ['film-one'])
  assert.deepEqual(visibleRealFilms(films, { ...defaults, source: 'source-a', sort: 'rating', direction: 'desc' }).map(film => film.id), ['film-one', 'film-three'])
  assert.deepEqual(visibleRealFilms(films, { ...defaults, sort: 'rating', direction: 'desc' }).map(film => film.id), ['film-one', 'film-three', 'film-two'])
  assert.deepEqual(visibleRealFilms(films, { ...defaults, query: '虚构乙' }).map(film => film.id), ['film-two'])
  assert.deepEqual(visibleRealFilms(films, { ...defaults, query: '乙.mp4' }), [])
  assert.deepEqual(visibleRealFilms(films, { ...defaults, query: '虚构来源甲' }), [])
})

test('bulk selection intersects explicitly chosen IDs with currently visible records', () => {
  const visible = visibleRealFilms(films, { ...defaults, favoritesOnly: true, selectedTags: ['已看'] })
  assert.deepEqual(selectedVisibleIds(['film-one', 'film-two', 'film-three'], visible), ['film-one'])
})

test('creation sorting uses saved folder timestamps with unknown times last in either direction', () => {
  const snapshots = [{...first, created_at:'2001-01-01T01:02:03.000Z'}, {...second, created_at:null}, {...third, created_at:'2002-02-02T04:05:06.000Z'}]
  assert.deepEqual(visibleRealFilms(snapshots, {...defaults,sort:'createdAt',direction:'asc'}).map(f=>f.id), ['film-one','film-three','film-two'])
  assert.deepEqual(visibleRealFilms(snapshots, {...defaults,sort:'createdAt',direction:'desc'}).map(f=>f.id), ['film-three','film-one','film-two'])
})


test('modification sorting uses folder timestamps independently of creation and keeps unknown last', () => {
  const snapshots = [{...first, created_at:'2001-01-01T00:00:00.000Z', modified_at:'2024-01-01T01:00:00.000Z'}, {...second, modified_at:null}, {...third, created_at:'2002-01-01T00:00:00.000Z', modified_at:'2023-01-01T02:00:00.000Z'}]
  assert.deepEqual(visibleRealFilms(snapshots, {...defaults,sort:'modifiedAt',direction:'asc'}).map(f=>f.id), ['film-three','film-one','film-two'])
  assert.deepEqual(visibleRealFilms(snapshots, {...defaults,sort:'modifiedAt',direction:'desc'}).map(f=>f.id), ['film-one','film-three','film-two'])
})

test('number sorting is natural and unknown numbers stay last in either direction', () => {
 const a={...first,videos:[{...first.videos[0],observed_path:'[TEST-10]/a.mp4'}]};const b={...second,videos:[{...second.videos[0],observed_path:'[TEST-2]/b.mp4'}]};
 assert.deepEqual(visibleRealFilms([a,b,third],{...defaults,sort:'code'}).map(f=>f.id),[b.id,a.id,third.id])
 assert.deepEqual(visibleRealFilms([a,b,third],{...defaults,sort:'code',direction:'desc'}).map(f=>f.id),[a.id,b.id,third.id])
})

test('number sorting reads directory evidence at most once per matching film', () => {
  let pathReads = 0
  const snapshots = Array.from({ length: 1000 }, (_, index): RealFilm => ({
    ...first, id: `film-${index}`, favorite: index % 2 === 0,
    videos: [{ ...first.videos[0], get observed_path() { pathReads++; return `[TEST-${(index * 7919) % 1000}]/video.mp4` } }],
  }))
  const visible = visibleRealFilms(snapshots, { ...defaults, sort: 'code', favoritesOnly: true })
  assert.equal(visible.length, 500)
  assert.ok(pathReads <= visible.length, `number sorting read ${pathReads} paths for ${visible.length} matching films`)
})

test('number sorting avoids rebuilding locale options for each comparison', () => {
  let optionComparisons = 0
  const original = String.prototype.localeCompare
  String.prototype.localeCompare = function (other, locales, options) {
    if (options?.numeric) optionComparisons++
    return original.call(this, other, locales, options)
  }
  try {
    const snapshots = Array.from({ length: 1000 }, (_, index) => ({ ...first, id: `film-${index}`, manual_code: `TEST-${(index * 7919) % 1000}` }))
    const visible = visibleRealFilms(snapshots, { ...defaults, sort: 'code' })
    assert.equal(visible.length, snapshots.length)
    assert.ok(optionComparisons <= 1, `number sorting rebuilt locale options ${optionComparisons} times`)
  } finally { String.prototype.localeCompare = original }
})

test('number sorting keeps base-sensitive ties, missing numbers and fresh edited values', () => {
  const snapshots = [
    { ...first, id: 'film-z', manual_code: 'test-02' },
    { ...first, id: 'film-a', manual_code: 'TEST-2' },
    { ...first, id: 'film-ten', manual_code: 'TEST-10' },
    { ...first, id: 'unknown-z', manual_code: null },
    { ...first, id: 'unknown-a', manual_code: null },
  ]
  assert.deepEqual(visibleRealFilms(snapshots, { ...defaults, sort: 'code' }).map(film => film.id), ['film-a', 'film-z', 'film-ten', 'unknown-a', 'unknown-z'])
  assert.deepEqual(visibleRealFilms(snapshots, { ...defaults, sort: 'code', direction: 'desc' }).map(film => film.id), ['film-ten', 'film-a', 'film-z', 'unknown-a', 'unknown-z'])
  snapshots[0].manual_code = 'TEST-1'
  assert.deepEqual(visibleRealFilms(snapshots, { ...defaults, sort: 'code' }).map(film => film.id), ['film-z', 'film-a', 'film-ten', 'unknown-a', 'unknown-z'])
})
