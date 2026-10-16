export function playerTime(seconds: number, duration = seconds) {
  const value = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0
  const hours = Math.floor(value / 3600), minutes = Math.floor(value / 60) % 60, rest = value % 60
  const hourDigits = Number.isFinite(duration) && duration >= 3600 ? String(Math.floor(duration / 3600)).length : 0
  return `${hours || hourDigits ? `${String(hours).padStart(hourDigits, '0')}:` : ''}${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`
}

export function playerKeyAction(code: string): { action: string; value?: string } | null {
  if (code === 'Backspace') return { action: 'back' }
  if (code === 'Space') return { action: 'pause' }
  if (code === 'Enter' || code === 'NumpadEnter') return { action: 'fullscreen' }
  if (code === 'ArrowLeft' || code === 'ArrowRight') return { action: 'skip', value: code === 'ArrowLeft' ? '-20' : '20' }
  return null
}

export function adjacentVideoId(videos: { id: string }[], current: string, direction: -1 | 1) {
  const index = videos.findIndex(video => video.id === current)
  return index < 0 ? null : videos[index + direction]?.id ?? null
}
// A response from before a replacement/close must never undo a newer response.
export function createPlayerReader<T>(read: () => Promise<T>, apply: (value: T) => void) {
  let request = 0
  return Object.assign(async () => { const current = ++request; const value = await read(); if (request === current) apply(value) }, { invalidate: () => { request++ } })
}
