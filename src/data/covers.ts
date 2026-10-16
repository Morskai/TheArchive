import { invoke } from './diagnostics.ts'
import { CoverCache } from './coverCache'

export function coverMessage(error: unknown) {
  switch (String(error)) {
    case 'cover_missing': return ''
    case 'cover_needs_review': return '固定封面归属待核对'
    case 'cover_too_large': return '图片过大，暂用占位图'
    case 'cover_invalid': return '图片无法解码，暂用占位图'
    case 'cover_unavailable': return '图片暂不可用'
    case 'cover_not_owned': return '图片归属需核对'
    case 'cover_cache_failed': return '缩略图暂无法生成'
    default: return '封面暂无法读取'
  }
}

// Bound IPC work. Components leaving the viewport cancel queued requests before they start.
// Shares successful thumbnails across scans. Explicit cache clearing/source removal
// invalidates them. Session retention has no fixed size cap; explicit clearing still releases owned resources.
export const coverCache = new CoverCache(
  (id, large) => {
    if (id.startsWith('film-version:')) {
      const [filmId, videoId] = JSON.parse(id.slice(13)) as string[]
      return invoke<ArrayBuffer>('load_version_cover_thumbnail', { filmId, videoId })
    }
    if (id.startsWith('film-actor:')) {
      const [filmId, videoId, actorId] = JSON.parse(id.slice(11)) as string[]
      return invoke<ArrayBuffer>('load_film_actor_thumbnail', { filmId, videoId, actorId })
    }
    return id.startsWith('actor:')
      ? invoke<ArrayBuffer>('load_actor_thumbnail', { actorId: id.slice(6), large })
      : invoke<ArrayBuffer>('load_cover_thumbnail', { filmId: id, large })
  },
  bytes => URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' })),
  url => URL.revokeObjectURL(url),
  { prepare: async (url, large) => {
    const image = new Image()
    image.src = url
    try {
      await image.decode()
      const side = large ? 1200 : 480
      if (!image.naturalWidth || !image.naturalHeight || image.naturalWidth > side || image.naturalHeight > side) throw 'cover_invalid'
      // Hold the decoded resource for the session, even when all views unmount.
      return { decodedBytes: image.naturalWidth * image.naturalHeight * 4, dispose: () => { image.src = '' } }
    } catch (error) { image.src = ''; throw error }
  } },
)
export function invalidateCovers() {
  coverCache.invalidate()
  window.dispatchEvent(new Event('real-covers-invalidated'))
}

export function invalidateChangedCovers(ids: string[]) {
  const filmIds = coverCache.invalidateIds(ids)
  if (filmIds.length) window.dispatchEvent(new CustomEvent('real-covers-invalidated', { detail: { filmIds } }))
}


export function refreshMissingCovers(scope: string[]) {
  const filmIds = coverCache.refreshMissing(scope)
  if (filmIds.length) window.dispatchEvent(new CustomEvent('real-covers-invalidated', { detail: { filmIds } }))
}
