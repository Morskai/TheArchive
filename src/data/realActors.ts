import { invoke } from './diagnostics.ts'
import { actorManualReads, filmManualReads, actorFavoriteSaved } from './manualEvents'
import { favoriteReads } from './filmFavoriteEvents'
import type { Actor } from './actors'
import type { Film } from './films'
import type { RealFilm } from './realFilms'
import { filmDisplayCode, filmSourceNames, hasAmbiguousFilmCode } from './realFilmPresentation'
import { createManualReadGuard } from './manualState'
import type { ManagedActorInput } from './actorManagementState'
import { invalidateChangedCovers } from './covers'
import { notifyRealLibraryChanged } from './playback'

export type ActorProfile = Pick<Actor, 'birthMonth' | 'height' | 'cup' | 'bust' | 'waist' | 'hips'>
export type ActorCatalog = { actors: (Actor & { hasPortrait: boolean; filmIds: string[] })[]; films: RealFilm[]; tags: string[]; actorTags?: string[] }
const managedActorReads = createManualReadGuard<Actor>()
export function actorManagedSaved(id: string, input: ManagedActorInput) {
  managedActorReads.commit(id, { name: input.name, aliases: input.aliases.join('、'), ...input.profile, filmIds: input.filmIds, ...(input.photoPath ? { hasPortrait: true } : input.removePhoto ? { hasPortrait: false } : {}) })
}
export const loadActorCatalog = () => managedActorReads.read(() => favoriteReads.read(() => actorManualReads.read(() => filmManualReads.read(() => invoke<ActorCatalog>('load_actor_catalog'), (catalog, patch) => ({ ...catalog, films: patch(catalog.films) })), (catalog, patch) => ({ ...catalog, actors: patch(catalog.actors) })), (catalog, patch) => ({ ...catalog, films: patch(catalog.films) })), (catalog, patch) => ({ ...catalog, actors: patch(catalog.actors) }))
export const setActorFavorite = async (actorId: string, favorite: boolean) => { await invoke<void>('set_actor_favorite', { actorId, favorite }); actorFavoriteSaved(actorId, favorite) }
export const saveActorProfile = (actorId: string, profile: ActorProfile, aliases?: string | null) => invoke<void>('save_actor_profile', { actorId, profile, aliases: aliases ?? null })
let deletingActors = false
export async function deleteActors(actorIds: string[]) {
  const ids = [...new Set(actorIds)]
  if (!ids.length) throw 'no_actors_selected'
  if (deletingActors) throw 'actor_delete_pending'
  deletingActors = true
  try {
    const count = await invoke<number>('delete_actors', { actorIds: ids })
    invalidateChangedCovers(ids.map(id => `actor:${id}`))
    notifyRealLibraryChanged()
    return count
  } finally { deletingActors = false }
}

export function actorDeleteMessage(error: unknown) {
  if (String(error) === 'protection_failed') return '资料保护失败，所选演员未删除。请检查软件资料目录后重试。'
  if (String(error) === 'actor_not_found') return '所选演员已发生变化，本次未删除。请刷新后重新选择。'
  return '演员未删除，所选资料保持不变，请重试。'
}
export function actorSaveMessage(error: unknown) {
  if (String(error) === 'actor_duplicate_identity') return '姓名和别名与另一位演员完全重复，请调整后再保存。'
  return String(error) === 'invalid_actor_profile' ? '个人信息格式或范围不正确，请检查后重试。' : String(error) === 'protection_failed' ? '资料保护备份失败，演员资料未更新。' : '演员资料未保存，请重试。'
}
export function actorFilmView(film: RealFilm): Film {
  return { id: film.id, realFilmId: film.id, actorIds: film.actors?.map(actor => actor.id) ?? [], actor: film.actors?.map(actor => actor.name).join('、') ?? '', title: film.title, code: filmDisplayCode(film), codePlaceholder: hasAmbiguousFilmCode(film), library: filmSourceNames(film), sources: [...new Map(film.videos.map(video => [video.source_id, { id: video.source_id, name: video.source_name }])).values()], rating: film.rating, favorite: film.favorite, tags: film.tags, hasCover: film.has_cover ?? false, artwork: 0, createdAt: null, lastLaunchedAt: film.last_launched_at ?? null, note: '', fileSize: null, directory: null, versions: [], preferredVersionId: film.preferred_video_id ?? null }
}
