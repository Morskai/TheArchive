export type FilmRatio = 'original' | '16:9' | '5:7'
export type ActorRatio = 'original' | '3:4' | '2:3'
export interface DisplayPreferences { defaultFilmRatio: FilmRatio; actorRatio: ActorRatio; libraryRatios: Record<string, FilmRatio> }
export const filmRatios: readonly FilmRatio[] = ['original', '16:9', '5:7']
export const actorRatios: readonly ActorRatio[] = ['original', '3:4', '2:3']
export const ratioLabel = (value: FilmRatio | ActorRatio, actor = false) => value === 'original' ? actor ? '5:7' : '800:543' : value
export const ratioCss = (value: FilmRatio | ActorRatio, actor = false) => ratioLabel(value, actor).replace(':', ' / ')
export const nextRatio = <T extends string>(values: readonly T[], current: T) => values[(values.indexOf(current) + 1) % values.length]
export function filmRatioForSources(ids: string[], selected: string, preferences: DisplayPreferences, overviewRatio?: FilmRatio): FilmRatio {
  if (selected === 'all' && overviewRatio) return overviewRatio
  const source = selected === 'all' ? [...new Set(ids)].sort()[0] : selected
  return preferences.libraryRatios[source] ?? preferences.defaultFilmRatio
}
export function currentFilmRatio(ids: string[], selected: string, preferences: DisplayPreferences, overviewRatio?: FilmRatio): FilmRatio | 'mixed' {
  if (selected !== 'all') return filmRatioForSources(ids, selected, preferences)
  if (overviewRatio) return overviewRatio
  const ratios = new Set(ids.map(id => preferences.libraryRatios[id] ?? preferences.defaultFilmRatio))
  return ratios.size > 1 ? 'mixed' : [...ratios][0] ?? preferences.defaultFilmRatio
}
