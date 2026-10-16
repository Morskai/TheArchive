import type { RealFilm } from './realFilms'
import { filmDisplayCode } from './realFilmPresentation.ts'
import { titleSearchText } from './titleTranslation.ts'
import { eventDate } from './localDates.ts'

export type RatingFilter = 'all' | '5' | '4' | '3' | '2' | '1'
export type RealSort = 'code' | 'title' | 'source' | 'rating' | 'createdAt' | 'modifiedAt' | 'lastLaunchedAt'
const codeCollator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' })

export interface RealFilmFilters {
  query: string
  source: string
  rating: RatingFilter
  favoritesOnly: boolean
  selectedTags: string[]
  sort: RealSort
  direction: 'asc' | 'desc'
}

export function visibleRealFilms(films: RealFilm[], filters: RealFilmFilters) {
  const term = filters.query.trim().toLocaleLowerCase()
  const visible = films.filter(film =>
    (filters.source === 'all' || film.videos.some(video => video.source_id === filters.source))
    && (filters.rating === 'all' || film.rating === Number(filters.rating))
    && (!filters.favoritesOnly || film.favorite)
    && filters.selectedTags.every(tag => film.tags.includes(tag))
    && (!term || titleSearchText(film).toLocaleLowerCase().includes(term))
  )
  // Folder parsing belongs to this filtered snapshot, not every sort comparison.
  const codes = filters.sort === 'code' ? new Map(visible.map(film => [film, filmDisplayCode(film)])) : null
  return visible.sort((a, b) => {
    if (filters.sort === 'code') {
      const left = codes!.get(a)!, right = codes!.get(b)!
      if (!left) return !right ? a.id.localeCompare(b.id) : 1
      if (!right) return -1
      const compared = codeCollator.compare(left, right)
      return (filters.direction === 'asc' ? compared : -compared) || a.id.localeCompare(b.id)
    }
    if (filters.sort === 'createdAt' || filters.sort === 'modifiedAt' || filters.sort === 'lastLaunchedAt') {
      const key = filters.sort === 'createdAt' ? 'created_at' : filters.sort === 'modifiedAt' ? 'modified_at' : 'last_launched_at'
      const left = a[key], right = b[key]
      if (!left) return !right ? a.id.localeCompare(b.id) : 1
      if (!right) return -1
      const compared = eventDate(left).getTime() - eventDate(right).getTime()
      return (filters.direction === 'asc' ? compared : -compared) || a.id.localeCompare(b.id)
    }
    if (filters.sort === 'rating') {
      if (a.rating === null) return b.rating === null ? a.id.localeCompare(b.id) : 1
      if (b.rating === null) return -1
      const compared = a.rating - b.rating
      return (filters.direction === 'asc' ? compared : -compared) || a.id.localeCompare(b.id)
    }
    const left = filters.sort === 'source' ? a.videos[0]?.source_name ?? '' : a.title
    const right = filters.sort === 'source' ? b.videos[0]?.source_name ?? '' : b.title
    const compared = left.localeCompare(right, 'zh-CN') || a.id.localeCompare(b.id)
    return filters.direction === 'asc' ? compared : -compared
  })
}

export function selectedVisibleIds(selectedIds: string[], visible: RealFilm[]) {
  const visibleIds = new Set(visible.map(film => film.id))
  return selectedIds.filter(id => visibleIds.has(id))
}
