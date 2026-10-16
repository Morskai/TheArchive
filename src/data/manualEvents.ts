import { createManualReadGuard } from './manualState'
import type { FilmCastActor } from './filmCast'
export type FilmManualPatch = { rating: number | null; preferred_video_id: string | null; actors: FilmCastActor[]; tags: string[] }
export const filmManualReads = createManualReadGuard<FilmManualPatch>()
export const actorManualReads = createManualReadGuard<{ favorite: boolean }>()
export function filmManualSaved(id: string, patch: Partial<FilmManualPatch>) {
  filmManualReads.commit(id, patch)
  window.dispatchEvent(new CustomEvent('film-manual-saved', { detail: { id, patch } }))
}
export function actorFavoriteSaved(id: string, favorite: boolean) {
  actorManualReads.commit(id, { favorite })
  window.dispatchEvent(new CustomEvent('actor-favorite-saved', { detail: { id, favorite } }))
}
export function listenFilmManualSaved(listener: (id: string, patch: Partial<FilmManualPatch>) => void) {
  const receive = (event: Event) => { const { id, patch } = (event as CustomEvent<{ id: string; patch: Partial<FilmManualPatch> }>).detail; listener(id, patch) }
  window.addEventListener('film-manual-saved', receive)
  return () => window.removeEventListener('film-manual-saved', receive)
}
export function listenActorFavoriteSaved(listener: (id: string, favorite: boolean) => void) {
  const receive = (event: Event) => { const { id, favorite } = (event as CustomEvent<{ id: string; favorite: boolean }>).detail; listener(id, favorite) }
  window.addEventListener('actor-favorite-saved', receive)
  return () => window.removeEventListener('actor-favorite-saved', receive)
}
