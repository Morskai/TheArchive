/** Repeated cache reports must not invalidate an unchanged catalogue snapshot. */
export function updateArtworkAvailability<T extends { id: string }, K extends keyof T>(
  items: T[], id: string, field: K, available: T[K],
): T[] {
  if (!items.some(item => item.id === id && item[field] !== available)) return items
  return items.map(item => item.id === id && item[field] !== available ? { ...item, [field]: available } : item)
}
