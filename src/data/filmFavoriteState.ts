export interface FavoriteSnapshot { favorite: boolean; saving: boolean; error: boolean }

// One writer per film. Intermediate clicks collapse to the latest requested value.
export function createFavoriteQueue(initial: boolean, persist: (value: boolean) => Promise<void>) {
  let committed = initial
  let state: FavoriteSnapshot = { favorite: initial, saving: false, error: false }
  const listeners = new Set<() => void>()
  const publish = (next: FavoriteSnapshot) => { state = next; listeners.forEach(listener => listener()) }
  const drain = async () => {
    try {
      while (state.favorite !== committed) {
        const target = state.favorite
        await persist(target)
        committed = target
      }
      publish({ favorite: committed, saving: false, error: false })
    } catch {
      publish({ favorite: committed, saving: false, error: true })
    }
  }
  return {
    snapshot: () => state,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener) } },
    toggle: () => {
      const running = state.saving
      publish({ favorite: !state.favorite, saving: true, error: false })
      if (!running) void drain()
    },
    syncSaved: (value: boolean) => {
      if (state.saving || state.favorite === value) return
      committed = value
      publish({ favorite: value, saving: false, error: false })
    },
  }
}

type FavoriteRow = { id: string; favorite: boolean }
type PatchRows = <T extends FavoriteRow>(rows: T[]) => T[]

// Retain only changes needed by reads currently in flight, then release them.
export function createFavoriteReadGuard() {
  let revision = 0
  const reads = new Set<{ revision: number }>()
  const patches = new Map<string, { favorite: boolean; revision: number }>()
  return {
    commit: (id: string, favorite: boolean) => {
      revision++
      if (reads.size) patches.set(id, { favorite, revision })
    },
    read: async <T>(load: () => Promise<T>, apply: (result: T, patch: PatchRows) => T): Promise<T> => {
      const read = { revision }
      reads.add(read)
      try {
        const result = await load()
        return apply(result, rows => rows.map(row => {
          const patch = patches.get(row.id)
          return patch && patch.revision > read.revision && row.favorite !== patch.favorite ? { ...row, favorite: patch.favorite } : row
        }))
      } finally {
        reads.delete(read)
        const oldest = Math.min(...[...reads].map(item => item.revision))
        for (const [id, patch] of patches) if (patch.revision <= oldest) patches.delete(id)
      }
    },
  }
}
