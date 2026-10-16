import type { Film } from './films'

export interface LaunchEvent {
  id: string
  filmId: string
  launchedAt: string
}

export interface HistorySource {
  listLaunchEvents(): Promise<LaunchEvent[]>
}

// Fictional successful-launch events for preview interactions only.
const demoEventRows = [
  ['e01', 'f01', 0, '20:18'],
  ['e02', 'f04', 0, '12:34'],
  ['e03', 'f04', 1, '21:06'],
  ['e04', 'f01', 1, '16:42'],
  ['e05', 'f05', 2, '19:28'],
  ['e06', 'f08', 3, '22:11'],
  ['e07', 'f10', 4, '18:53'],
  ['e08', 'f01', 5, '20:05'],
  ['e09', 'f04', 6, '14:26'],
  ['e10', 'f02', 2, '10:48'],
  ['e11', 'f07', 3, '09:16'],
  ['e12', 'f03', 4, '21:17'],
  ['e13', 'f11', 5, '14:37'],
] as const

function localDay(daysAgo: number) {
  const date = new Date()
  date.setDate(date.getDate() - daysAgo)
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

export const demoHistorySource: HistorySource = {
  async listLaunchEvents() {
    return demoEventRows.map(([id, filmId, daysAgo, time]) => ({ id, filmId, launchedAt: `${localDay(daysAgo)}T${time}:00` }))
  },
}

export function filmsWithLaunchHistory(films: Film[], events: LaunchEvent[]): Film[] {
  const latestByFilm = new Map<string, string>()
  events.forEach(event => {
    const previous = latestByFilm.get(event.filmId)
    if (!previous || event.launchedAt > previous) latestByFilm.set(event.filmId, event.launchedAt)
  })
  return films.map(film => {
    const latest = latestByFilm.get(film.id)
    return {
      ...film,
      lastLaunchedAt: latest ? latest.slice(0, 10) : null,
      versions: film.versions.map(version => version.id === `${film.id}-main` ? { ...version, note: latest ? '最近有启动记录' : '暂无启动记录' } : version),
    }
  })
}
