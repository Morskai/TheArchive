// Optimistic single writer: intermediate requests collapse to the newest value.
export function createSavedValueQueue<T>(initial: T, persist: (value: T, committed: T) => Promise<void>) {
  let committed = initial
  let state = { value: initial, saving: false, error: false }
  const listeners = new Set<() => void>()
  const publish = (next: typeof state) => { state = next; listeners.forEach(listener => listener()) }
  const drain = async () => {
    try {
      while (!Object.is(state.value, committed)) {
        const target = state.value
        await persist(target, committed)
        committed = target
      }
      publish({ value: committed, saving: false, error: false })
    } catch { publish({ value: committed, saving: false, error: true }) }
  }
  return {
    snapshot: () => state,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener) } },
    set: (value: T) => {
      if (Object.is(value, state.value) && !state.error) return
      const running = state.saving
      publish({ value, saving: true, error: false })
      if (!running) void drain()
    },
    syncSaved: (value: T) => {
      if (state.saving || Object.is(value, state.value)) return
      committed = value; publish({ value, saving: false, error: false })
    },
  }
}

// A stale catalog read cannot overwrite writes committed after that read began.
export function createManualReadGuard<P extends object>() {
  let revision = 0
  const reads = new Set<number>()
  const patches = new Map<string, Map<keyof P, { value: unknown; revision: number }>>()
  return {
    commit(id: string, patch: Partial<P>) {
      revision++
      if (!reads.size) return
      const fields = patches.get(id) ?? new Map()
      for (const key of Object.keys(patch) as (keyof P)[]) fields.set(key, { value: patch[key], revision })
      patches.set(id, fields)
    },
    async read<R>(load: () => Promise<R>, apply: (result: R, patch: <T extends { id: string }>(rows: T[]) => T[]) => R) {
      const start = revision
      // Multiple reads can start at the same revision.
      const token = ++revision
      reads.add(token)
      try {
        return apply(await load(), rows => rows.map(row => {
          const fields = patches.get(row.id)
          const patch: Record<string, unknown> = {}
          fields?.forEach((field, key) => { if (field.revision > start) patch[String(key)] = field.value })
          return Object.keys(patch).length ? { ...row, ...patch } : row
        }))
      } finally {
        reads.delete(token)
        const oldest = Math.min(...reads)
        for (const [id, fields] of patches) {
          for (const [key, field] of fields) if (field.revision <= oldest) fields.delete(key)
          if (!fields.size) patches.delete(id)
        }
      }
    },
  }
}
