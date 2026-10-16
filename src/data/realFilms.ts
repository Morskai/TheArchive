import { isTauri } from '@tauri-apps/api/core'
import { invoke } from './diagnostics.ts'
import { filmManualReads, filmManualSaved } from './manualEvents'
import { favoriteReads, favoriteSaved } from './filmFavoriteEvents'
import type { FilmCastActor, FilmCastCandidate } from './filmCast'
import { notifyRealLibraryChanged } from './playback'
export type { FilmCastActor, FilmCastCandidate } from './filmCast'

export interface RealVideo {
  id: string
  observed_path: string
  source_id: string
  source_name: string
  directory_path?: string
}

export interface RealFilm {
  actors?: FilmCastActor[]
  has_cover?: boolean
  preferred_video_id?: string | null
  last_launched_at?: string | null
  created_at?: string | null
  modified_at?: string | null
  id: string
  title: string
  original_title?: string
  automatic_title?: string | null
  translation_stale?: boolean
  title_edited?: boolean
  manual_code: string | null
  needs_organization: boolean
  rating: number | null
  favorite: boolean
  tags: string[]
  videos: RealVideo[]
}

export const realFilmsAvailable = isTauri()
export const loadRealFilms = () => favoriteReads.read(() => filmManualReads.read(() => invoke<RealFilm[]>('load_real_films'), (rows, patch) => patch(rows)), (rows, patch) => patch(rows))
export const loadRealTags = () => invoke<string[]>('list_real_tags')
export const createRealTag = (name: string) => invoke<void>('create_real_tag', { name })
export const deleteRealTag = (name: string) => invoke<void>('delete_real_tag', { name })
export const deleteRealFilms = (filmIds: string[]) => invoke<number>('delete_real_films', { filmIds })
export const renameRealTag = (name: string, newName: string) => invoke<void>('rename_real_tag', { name, newName })
export const setRealFavorite = async (filmId: string, favorite: boolean) => {
  await invoke<void>('set_real_favorite', { filmId, favorite })
  favoriteSaved(filmId, favorite)
}
export const setRealRating = async (filmId: string, stars: number | null) => { await invoke<void>('set_real_rating', { filmId, stars }); filmManualSaved(filmId, { rating: stars }) }
export const setRealFilmTags = async (filmId: string, names: string[]) => { await invoke<void>('set_real_film_tags', { filmId, names }); filmManualSaved(filmId, { tags: [...names] }) }
export const applyRealBulkTag = (filmIds: string[], name: string, add: boolean) => invoke<number>('apply_real_bulk_tag', { filmIds, name, add })
export const listCastCandidates = (filmId: string) => invoke<FilmCastCandidate[]>('list_cast_candidates', { filmId })

// A drawer can close and reopen while a write finishes; retain one writer per film.
const pendingCast = new Map<string, { command: string; actorId: string; promise: Promise<FilmCastActor[]> }>()
function saveFilmCast(command: 'add_film_cast_actor' | 'remove_film_cast_actor', filmId: string, actorId: string) {
  const pending = pendingCast.get(filmId)
  if (pending) return pending.command === command && pending.actorId === actorId ? pending.promise : Promise.reject('cast_save_pending')
  const promise = invoke<FilmCastActor[]>(command, { filmId, actorId }).then(actors => {
    filmManualSaved(filmId, { actors })
    // Actor membership and derived statistics need the updated catalog too.
    notifyRealLibraryChanged()
    return actors
  }).finally(() => { pendingCast.delete(filmId) })
  pendingCast.set(filmId, { command, actorId, promise })
  return promise
}
export const addFilmCastActor = (filmId: string, actorId: string) => saveFilmCast('add_film_cast_actor', filmId, actorId)
export const removeFilmCastActor = (filmId: string, actorId: string) => saveFilmCast('remove_film_cast_actor', filmId, actorId)

export const reorderRealTags = (names: string[]) => invoke<void>('reorder_real_tags', { names })
