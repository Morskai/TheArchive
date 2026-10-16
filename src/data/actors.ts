export interface Actor {
  id: string
  realActorId?: string
  hasPortrait?: boolean
  sources?: { id: string; name: string }[]
  filmIds?: string[]
  tags?: string[]
  drawable?: boolean
  name: string
  aliases?: string | null
  directoryName: string
  library: string
  directoryCreatedAt: string | null
  directoryModifiedAt?: string | null
  favorite: boolean
  portrait: number
  birthMonth: string
  height: string
  cup: string
  bust: string
  waist: string
  hips: string
}

export interface ActorFilters {
  query: string
  favoritesOnly: boolean
  selectedTags: string[]
  missingPhotosOnly?: boolean
}

export function filterActors(actors: Actor[], filters: ActorFilters): Actor[] {
  const term = filters.query.trim().toLocaleLowerCase()
  const found = new Set<string>()
  return actors.filter(actor => {
    if (found.has(actor.id)) return false
    const matches = (!term || [actor.name, actor.aliases ?? ''].some(value => value.toLocaleLowerCase().includes(term)))
      && (!filters.favoritesOnly || actor.favorite)
      && (!filters.missingPhotosOnly || actor.hasPortrait === false)
      && filters.selectedTags.every(tag => (actor.tags ?? []).includes(tag))
    if (matches) found.add(actor.id)
    return matches
  })
}

export function actorSelectionScope(filters: ActorFilters): string {
  return JSON.stringify([filters.query, filters.favoritesOnly, [...filters.selectedTags].sort(), Boolean(filters.missingPhotosOnly)])
}

export function selectedActorIds(selection: { scope: string; ids: string[] }, scope: string, visible: { id: string }[]): string[] {
  if (selection.scope !== scope) return []
  const visibleIds = new Set(visible.map(actor => actor.id))
  return [...new Set(selection.ids)].filter(id => visibleIds.has(id))
}

// Fictional entries for interface review; no actor directory is read.
export const demoActors: Actor[] = [
  { id: 'a01', name: '林澄', directoryName: '林澄（主收藏）', library: '主收藏', directoryCreatedAt: '2025-06-18', favorite: true, portrait: 1, birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' },
  { id: 'a02', name: '沈弥', directoryName: '沈弥（精选）', library: '主收藏', directoryCreatedAt: '2025-09-12', favorite: true, portrait: 2, birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' },
  { id: 'a03', name: '周遥', directoryName: '周遥', library: '主收藏', directoryCreatedAt: '2024-11-06', favorite: false, portrait: 3, birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' },
  { id: 'a04', name: '苏梨', directoryName: '苏梨（备份）', library: '备用片库', directoryCreatedAt: null, favorite: false, portrait: 4, birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' },
  { id: 'a05', name: '叶岚', directoryName: '叶岚', library: '主收藏', directoryCreatedAt: '2025-03-08', favorite: false, portrait: 5, birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' },
  { id: 'a06', name: '唐宁', directoryName: '唐宁', library: '主收藏', directoryCreatedAt: '2024-08-23', favorite: false, portrait: 6, birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' },
  { id: 'a07', name: '许知遥', directoryName: '许知遥', library: '备用片库', directoryCreatedAt: '2025-12-05', favorite: false, portrait: 7, birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' },
  { id: 'a08', name: '顾晚', directoryName: '顾晚', library: '主收藏', directoryCreatedAt: '2026-01-17', favorite: false, portrait: 8, birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' },
]
