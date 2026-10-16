import { useEffect, useMemo, useSyncExternalStore } from 'react'
import { createFavoriteQueue } from '../data/filmFavoriteState'
import { setRealFavorite } from '../data/realFilms'

type Queue = ReturnType<typeof createFavoriteQueue>
const entries = new Map<string, { queue: Queue; subscribers: number; release: () => void }>()
const empty = createFavoriteQueue(false, async () => {})

function getEntry(id: string, saved: boolean) {
  const existing = entries.get(id)
  if (existing) return existing
  const queue = createFavoriteQueue(saved, value => setRealFavorite(id, value))
  const entry = { queue, subscribers: 0, release: () => {} }
  entry.release = queue.subscribe(() => {
    queueMicrotask(() => {
      if (!entry.subscribers && !queue.snapshot().saving && entries.get(id) === entry) { entry.release(); entries.delete(id) }
    })
  })
  entries.set(id, entry)
  return entry
}

export function useFilmFavorite(id: string | undefined, saved: boolean) {
  const entry = useMemo(() => id ? getEntry(id, saved) : null, [id])
  const queue = entry?.queue ?? empty
  const subscribe = useMemo(() => (listener: () => void) => {
    if (!entry || !id) return () => {}
    entry.subscribers++
    const stop = queue.subscribe(listener)
    return () => {
      stop(); entry.subscribers--
      queueMicrotask(() => {
        if (!entry.subscribers && !queue.snapshot().saving && entries.get(id) === entry) { entry.release(); entries.delete(id) }
      })
    }
  }, [entry, id, queue])
  const state = useSyncExternalStore(subscribe, queue.snapshot)
  useEffect(() => { queue.syncSaved(saved) }, [queue, saved])
  return { ...state, toggle: queue.toggle }
}
