// SQLite CURRENT_TIMESTAMP from older records is UTC without a suffix.
export function eventDate(value: string) {
  return new Date(/(?:Z|[+-]\d{2}:\d{2})$/i.test(value) ? value : `${value.replace(' ', 'T')}Z`)
}
export function localDateKey(date: Date) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}
export function localLaunchTime(value: string) {
  const date = eventDate(value)
  return `${localDateKey(date)} ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
}
