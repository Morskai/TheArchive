import assert from 'node:assert/strict'
import test from 'node:test'
import { eventDate, localDateKey, localLaunchTime } from '../src/data/localDates.ts'
import { visibleRealFilms } from '../src/data/realFilmFilters.ts'
import type { RealFilm } from '../src/data/realFilms'

test('UTC launches cross local midnight and legacy SQLite dates use the same local day', () => {
  const previous = process.env.TZ
  process.env.TZ = 'Asia/Shanghai'
  try {
    assert.equal(localDateKey(eventDate('2026-09-29T16:30:00.000Z')), '2026-09-30')
    assert.equal(localLaunchTime('2026-09-29 16:30:00'), '2026-09-30 00:30')
    assert.equal(localDateKey(eventDate('2026-09-29T15:59:00Z')), '2026-09-29')
  } finally { if (previous === undefined) delete process.env.TZ; else process.env.TZ = previous }
})

test('latest launch sorting uses saved timestamps and puts no-history films last', () => {
  const base: RealFilm = { id: 'none', title: '虚构', manual_code: null, rating: null, favorite: false, tags: [], videos: [], needs_organization: true }
  const films = [{ ...base, id: 'morning', last_launched_at: '2026-09-30T01:00:00Z' }, { ...base, id: 'evening', last_launched_at: '2026-09-30 15:00:00' }, base]
  assert.deepEqual(visibleRealFilms(films, { query: '', source: 'all', rating: 'all', favoritesOnly: false, selectedTags: [], sort: 'lastLaunchedAt', direction: 'desc' }).map(film => film.id), ['evening', 'morning', 'none'])
  assert.deepEqual(visibleRealFilms(films.map(film => ({ ...film, last_launched_at: null })), { query: '', source: 'all', rating: 'all', favoritesOnly: false, selectedTags: [], sort: 'lastLaunchedAt', direction: 'desc' }).length, 3)
})
