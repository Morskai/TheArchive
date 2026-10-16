import type { Film } from './films'

function sizeInGigabytes(size: string) {
  const match = size.match(/^(\d+(?:\.\d+)?)\s*(MB|GB|TB)$/i)
  if (!match) return 0
  const value = Number(match[1])
  const unit = match[2].toUpperCase()
  return unit === 'TB' ? value * 1024 : unit === 'MB' ? value / 1024 : value
}

export function libraryOverview(films: Film[]) {
  const sizeGb = films.flatMap(film => film.versions).reduce((total, version) => total + sizeInGigabytes(version.size), 0)
  return {
    filmCount: films.length,
    sizeLabel: sizeGb >= 1024 ? `${(sizeGb / 1024).toFixed(1)} TB` : `${sizeGb.toFixed(1)} GB`,
    sourceCount: new Set(films.map(film => film.library)).size,
  }
}
