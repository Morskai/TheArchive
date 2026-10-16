// Five full-width characters; Latin letters and digits use half a character width.
export function castNameLabel(name: string): string {
  let width = 0
  let result = ''
  for (const character of name) {
    const next = /^[\x20-\x7e]$/.test(character) ? .5 : 1
    if (width + next > 5) return result + '..'
    width += next
    result += character
  }
  return result
}

export function tagFilmCounts(films: { tags: string[] }[]): Record<string, number> {
  const counts: Record<string, number> = Object.create(null)
  for (const film of films) for (const tag of new Set(film.tags)) counts[tag] = (counts[tag] ?? 0) + 1
  return counts
}
export function moveTag(tags: string[], from: string, to: string): string[] {
  const start = tags.indexOf(from), end = tags.indexOf(to)
  if (start < 0 || end < 0 || start === end) return tags
  const next = [...tags]
  next.splice(start, 1)
  next.splice(end, 0, from)
  return next
}

// Insert relative to the drop target, independent of which side the source came from.
export function placeTag(tags: string[], from: string, to: string, before: boolean): string[] {
  if (from === to || !tags.includes(from) || !tags.includes(to)) return tags
  const next = tags.filter(tag => tag !== from)
  next.splice(next.indexOf(to) + (before ? 0 : 1), 0, from)
  return next.every((tag, index) => tag === tags[index]) ? tags : next
}
