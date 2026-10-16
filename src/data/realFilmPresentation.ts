import type { RealFilm, RealVideo } from './realFilms'

export function directoryNumber(video: RealVideo): string[] {
  const parts = video.observed_path.replaceAll('\\', '/').split('/')
  const directory = video.directory_path?.replaceAll('\\', '/').split('/').filter(Boolean).at(-1) ?? (parts.length > 1 ? parts.at(-2)! : video.source_name)
  const code = [...directory.matchAll(/\[([^\[\]]+)\]/g)].map(match => match[1].trim()).find(Boolean)
  return code ? [code] : []
}

export function filmDisplayCode(film: RealFilm) {
  if (film.manual_code?.trim()) return film.manual_code.trim()
  const codes = [...new Set(film.videos.flatMap(directoryNumber))]
  return codes.length === 1 ? codes[0] : codes.length > 1 ? '编号待核对' : ''
}

export const hasAmbiguousFilmCode = (film: RealFilm) => !film.manual_code?.trim() && new Set(film.videos.flatMap(directoryNumber)).size > 1

export const filmSourceNames = (film: RealFilm) => [...new Set(film.videos.map(video => video.source_name))].join('、')

export function videoNameAndFormat(path: string) {
  const name = path.replaceAll('\\', '/').split('/').at(-1) ?? path
  const dot = name.lastIndexOf('.')
  return { name: dot > 0 ? name.slice(0, dot) : name, format: dot > 0 ? name.slice(dot + 1).toUpperCase() : '格式未知' }
}

export function videoSizeLabel(bytes: number | undefined) {
  if (bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return '大小未知'
  const unit = bytes >= 1024 ** 3 ? 'GB' : 'MB'
  const size = bytes / (unit === 'GB' ? 1024 ** 3 : 1024 ** 2)
  return `${size.toFixed(1).replace(/\.0$/, '')}${unit}`
}
/** Use the same initial playback version as the detail drawer. */
export function defaultFilmVideo(film: { preferred_video_id?: string | null; videos: { id: string }[] }) {
  return film.videos.find(video => video.id === film.preferred_video_id)?.id ?? film.videos[0]?.id ?? null
}
