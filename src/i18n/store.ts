import { isLanguage, type Language } from './core.ts'

interface LanguageAdapter { load(): Promise<unknown>; save(language: Language): Promise<void> }
export interface LanguageSnapshot { language: Language; ready: boolean; busy: boolean; error: '' | 'load' | 'save' }

export function createLanguageStore(adapter: LanguageAdapter, preview = false) {
  let snapshot: LanguageSnapshot = { language: 'zh-CN', ready: preview, busy: false, error: '' }
  let revision = 0
  const listeners = new Set<() => void>()
  const publish = (next: LanguageSnapshot) => { snapshot = next; for (const listener of listeners) listener() }
  return {
    getSnapshot: () => snapshot,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener) } },
    accept(value: unknown) {
      if (!isLanguage(value)) return
      revision++
      publish({ language: value, ready: true, busy: false, error: '' })
    },
    async load() {
      if (snapshot.busy) return
      const request = ++revision
      publish({ ...snapshot, busy: true, error: '' })
      try {
        const language = await adapter.load()
        if (!isLanguage(language)) throw new Error('invalid_interface_language')
        if (request === revision) publish({ language, ready: true, busy: false, error: '' })
      } catch {
        if (request === revision) publish({ ...snapshot, busy: false, error: 'load' })
      }
    },
    async save(language: Language) {
      if (!snapshot.ready || snapshot.busy || !isLanguage(language) || snapshot.language === language) return
      const request = ++revision
      publish({ ...snapshot, busy: true, error: '' })
      try {
        await adapter.save(language)
        if (request === revision) publish({ language, ready: true, busy: false, error: '' })
      } catch {
        if (request === revision) publish({ ...snapshot, busy: false, error: 'save' })
      }
    },
  }
}
