import { useEffect, useMemo, useSyncExternalStore } from 'react'
import { createSavedValueQueue } from '../data/manualState'
// Entries remain alive while a write is pending, including drawer close/reopen.
type Entry = { queue: ReturnType<typeof createSavedValueQueue<unknown>>; subscribers: number; release: () => void }
const entries = new Map<string, Entry>()
export function useManualValue<T>(key: string, saved: T, persist: (value: T, committed: T) => Promise<void>) {
  const entry = useMemo(() => {
    const existing = entries.get(key)
    if (existing) return existing
    const queue = createSavedValueQueue<unknown>(saved, (value, committed) => persist(value as T, committed as T))
    const entry: Entry = { queue, subscribers: 0, release: () => {} }
    entry.release = queue.subscribe(() => queueMicrotask(() => {
      if (!entry.subscribers && !queue.snapshot().saving && entries.get(key) === entry) { entry.release(); entries.delete(key) }
    }))
    entries.set(key, entry)
    return entry
  }, [key])
  const { queue } = entry
  const subscribe = useMemo(() => (listener: () => void) => {
    entry.subscribers++
    const stop = queue.subscribe(listener)
    return () => {
      stop(); entry.subscribers--
      queueMicrotask(() => {
        if (!entry.subscribers && !queue.snapshot().saving && entries.get(key) === entry) { entry.release(); entries.delete(key) }
      })
    }
  }, [entry, key, queue])
  const state = useSyncExternalStore(subscribe, queue.snapshot)
  useEffect(() => { queue.syncSaved(saved) }, [queue, saved])
  return { ...state, value: state.value as T, set: queue.set as (value: T) => void }
}
