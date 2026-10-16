type Consumer = { active: () => boolean; priority: () => number }
type PreparedImage = { decodedBytes: number; dispose: () => void }
type Entry = {
  key: string; filmId: string; large: boolean; consumers: Set<Consumer>
  result: Promise<string | null>; resolve: (url: string | null) => void; reject: (error: unknown) => void
  url: string | null; settled: boolean; retired: boolean; bytes: number; prepared?: PreparedImage
}

/** Bounded session cache. URLs remain owned here, never by individual cards. */
export class CoverCache {
  private entries = new Map<string, Entry>()
  private queue: Entry[] = []
  private running = 0
  readonly budgetBytes: number
  private bytes = 0
  private listeners = new Map<string, Set<(url: string) => void>>()
  private load: (filmId: string, large: boolean) => Promise<ArrayBuffer>
  private create: (bytes: ArrayBuffer) => string
  private revoke: (url: string) => void
  private prepare?: (url: string, large: boolean) => Promise<PreparedImage>
  private failures = new Map<string, unknown>()
  private runningImages = new Set<string>()

  private imageKey(id: string) {
    return id.startsWith('film-actor:') ? `actor:${(JSON.parse(id.slice(11)) as string[])[2]}` : id
  }
  private ownerKey(id: string) {
    return id.startsWith('film-version:') ? (JSON.parse(id.slice(13)) as string[])[0] : this.imageKey(id)
  }

  constructor(load: (filmId: string, large: boolean) => Promise<ArrayBuffer>, create: (bytes: ArrayBuffer) => string, revoke: (url: string) => void, options: { maxBytes?: number; prepare?: (url: string, large: boolean) => Promise<PreparedImage> } = {}) {
    this.load = load; this.create = create; this.revoke = revoke
    this.prepare = options.prepare
    // Reserve both encoded bytes and a conservative RGBA surface per thumbnail.
    // This is a retention budget, not an allocation or a whole-process RAM limit.
    this.budgetBytes = options.maxBytes ?? Number.POSITIVE_INFINITY
  }

  get retainedBytes() { return this.bytes }

  /** A remounted view can paint an existing image before its visibility observer runs. Never starts a read. */
  peek(filmId: string, large: boolean) {
    const entry = this.entries.get(JSON.stringify([filmId, large]))
    return entry && !entry.retired ? entry.url : null
  }

  subscribe(filmId: string, large: boolean, listener: (url: string) => void) {
    const key = JSON.stringify([filmId, large])
    const listeners = this.listeners.get(key) ?? new Set()
    listeners.add(listener); this.listeners.set(key, listeners)
    return () => { listeners.delete(listener); if (!listeners.size) this.listeners.delete(key) }
  }

  acquire(filmId: string, large: boolean, active: () => boolean, priority: () => number) {
    const key = JSON.stringify([filmId, large])
    let entry = this.entries.get(key)
    if (!entry) {
      let resolve!: Entry['resolve'], reject!: Entry['reject']
      const result = new Promise<string | null>((yes, no) => { resolve = yes; reject = no })
      entry = { key, filmId, large, consumers: new Set(), result, resolve, reject, url: null, settled: false, retired: false, bytes: 0 }
      this.entries.set(key, entry)
      this.queue.push(entry)
    }
    // Map insertion order is the LRU order.
    this.entries.delete(key); this.entries.set(key, entry)
    const consumer = { active, priority }
    entry.consumers.add(consumer)
    const held = entry
    this.drain()
    let released = false
    return { url: entry.url, result: entry.result, release: () => {
      if (released) return
      released = true; held.consumers.delete(consumer)
      if (held.retired && !held.consumers.size) this.dispose(held)
      this.trim(); this.drain()
    } }
  }

  /** Scanning adds artwork; only failed reads need a retry. Successful URLs stay live. */
  refreshMissing(scope: string[]) {
    const selected = new Set(scope.map(id => this.imageKey(id)))
    const ids = new Set<string>()
    for (const id of this.failures.keys()) if (selected.has(this.ownerKey(id)) && this.failures.delete(id)) ids.add(id)
    for (const entry of [...this.entries.values()]) {
      if (selected.has(this.ownerKey(entry.filmId)) && entry.settled && !entry.url) { ids.add(entry.filmId); this.retire(entry) }
    }
    return [...ids]
  }

  invalidate() { this.failures.clear(); for (const entry of [...this.entries.values()]) this.retire(entry) }

  invalidateIds(ids: string[]) {
    const selected = new Set(ids)
    const affected = new Set(ids)
    for (const id of this.failures.keys()) if (selected.has(this.ownerKey(id))) this.failures.delete(id)
    for (const entry of [...this.entries.values()]) {
      let matches = selected.has(entry.filmId) || selected.has(this.ownerKey(entry.filmId))
      if (entry.filmId.startsWith('film-actor:')) {
        const actorId = (JSON.parse(entry.filmId.slice(11)) as string[])[2]
        matches ||= selected.has(`actor:${actorId}`)
      }
      if (matches) { affected.add(entry.filmId); this.retire(entry) }
    }
    return [...affected]
  }

  private dispose(entry: Entry) {
    if (entry.url) {
      const url = entry.url
      entry.prepared?.dispose(); entry.prepared = undefined
      entry.url = null; this.bytes -= entry.bytes; entry.bytes = 0
      for (const listener of this.listeners.get(entry.key) ?? []) listener(url)
      this.revoke(url)
    }
  }
  private retire(entry: Entry) {
    entry.retired = true
    if (this.entries.get(entry.key) === entry) this.entries.delete(entry.key)
    if (!entry.consumers.size) this.dispose(entry)
  }
  private trim() {
    for (const entry of this.entries.values()) {
      if (this.bytes <= this.budgetBytes) break
      if (!entry.settled || entry.consumers.size) continue
      this.retire(entry)
    }
  }
  private priority(entry: Entry) {
    return Math.min(...[...entry.consumers].filter(consumer => consumer.active()).map(consumer => consumer.priority()))
  }
  private drain() {
    this.queue.sort((a, b) => this.priority(a) - this.priority(b))
    while (this.running < 2 && this.queue.length) {
      const position = this.queue.findIndex(entry => !this.runningImages.has(this.imageKey(entry.filmId)))
      if (position === -1) break
      const entry = this.queue.splice(position, 1)[0]
      if (entry.retired || this.priority(entry) === Infinity) {
        this.retire(entry); entry.settled = true; entry.resolve(null); continue
      }
      this.running++
      this.runningImages.add(this.imageKey(entry.filmId))
      void this.run(entry)
    }
  }
  private async run(entry: Entry) {
    let failure: unknown, failed = false
    try {
      if (this.failures.has(this.imageKey(entry.filmId))) throw this.failures.get(this.imageKey(entry.filmId))
      const bytes = await this.load(entry.filmId, entry.large)
      if (!entry.retired) {
        const url = this.create(bytes)
        let prepared: PreparedImage | undefined
        try { prepared = await this.prepare?.(url, entry.large) }
        catch (error) { this.revoke(url); throw error }
        if (entry.retired) { prepared?.dispose(); this.revoke(url) }
        else {
          entry.url = url; entry.prepared = prepared
          const side = entry.large ? 1200 : 480
          entry.bytes = bytes.byteLength + (prepared?.decodedBytes ?? side * side * 4)
          this.bytes += entry.bytes
        }
      }
    } catch (error) {
      failed = true; failure = error
      if (!entry.retired) this.failures.set(this.imageKey(entry.filmId), error)
    }
    entry.settled = true; this.running--
    this.runningImages.delete(this.imageKey(entry.filmId))
    if (entry.retired && !entry.consumers.size) this.dispose(entry)
    this.trim(); this.drain()
    if (failed) entry.reject(failure)
    else entry.resolve(entry.retired ? null : entry.url)
  }
}
