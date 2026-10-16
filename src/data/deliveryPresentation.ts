export function actorAliases(directoryName: string) {
  return [...directoryName.matchAll(/\(([^()]*)\)|（([^（）]*)）/g)].map(match => (match[1] ?? match[2]).trim()).filter(Boolean).join('、')
}
export function sourceSizeLabel(bytes: number | null | undefined) {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return '未记录'
  return bytes >= 1e12 ? `${(bytes / 1e12).toFixed(1)} TB` : `${(bytes / 1e9).toFixed(1)} GB`
}
export interface VideoResolution { width: number; height: number }
export function videoResolutionLabel(value: VideoResolution | null | undefined) {
  return value && Number.isInteger(value.width) && Number.isInteger(value.height) && value.width > 0 && value.height > 0 ? ` · ${value.width}*${value.height}` : ''
}


export function actorPhotoSummary(actors: { hasPortrait?: boolean }[]) {
  const missing = actors.filter(actor => actor.hasPortrait === false).length
  return { missing, percent: actors.length ? Math.round((actors.length - missing) / actors.length * 100) : 0 }
}
/** Count distinct local calendar days in the inclusive window ending today. */
export function activePlaybackDays(days: Iterable<string>, now = new Date()) {
  const key = (date: Date) => `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
  const first = new Date(now); first.setDate(first.getDate() - 29)
  const start = key(first), end = key(now)
  return new Set([...days].filter(day => /^\d{4}-\d{2}-\d{2}$/.test(day) && day >= start && day <= end)).size
}
