import { readFileSync, existsSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'

// Execute the actual App/library component bodies and committed effects without
// a browser or native app. Only React scheduling, untouched UI children and IPC
// are test boundaries; catalog loaders/read guards/events are production code.
export function catalogComponents(initialCatalog: unknown) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
  const nodeRequire = createRequire(import.meta.url)
  const commands: string[] = []
  const calls: { command: string; args: any }[] = []
  const pending: { resolve: (catalog: unknown) => void; reject: (error: unknown) => void }[] = []
  const pendingHistory: { resolve: (history: unknown) => void; reject: (error: unknown) => void }[] = []
  let deferCatalog = false
  let deferHistory = false
  const listeners = new Map<string, Set<(event: Event) => void>>()
  const window = {
    matchMedia: () => ({ matches: false }),
    addEventListener(name: string, callback: (event: Event) => void) { const current = listeners.get(name) ?? new Set(); current.add(callback); listeners.set(name, current) },
    removeEventListener(name: string, callback: (event: Event) => void) { listeners.get(name)?.delete(callback) },
    dispatchEvent(event: Event) { for (const callback of listeners.get(event.type) ?? []) callback(event) },
  }
  const document = { title: '', documentElement: { dataset: {}, style: { setProperty() {} } } }
  type Runner = { slots: any[]; cursor: number; dirty: boolean; effects: (() => void)[]; output: any; render: (props?: any) => void; dispose: () => void }
  let current: Runner
  const equal = (a: unknown[] | undefined, b: unknown[] | undefined) => a && b && a.length === b.length && a.every((value, index) => Object.is(value, b[index]))
  const useMemo = (callback: () => unknown, deps: unknown[]) => {
    const runner = current, index = runner.cursor++, old = runner.slots[index]
    if (!old || !equal(old.deps, deps)) runner.slots[index] = { value: callback(), deps }
    return runner.slots[index].value
  }
  const useEffect = (callback: () => void | (() => void), deps?: unknown[]) => {
    const runner = current, index = runner.cursor++, old = runner.slots[index]
    if (old && equal(old.deps, deps)) return
    const slot = { deps, cleanup: old?.cleanup }
    runner.slots[index] = slot
    runner.effects.push(() => { slot.cleanup?.(); slot.cleanup = callback() })
  }
  const react = {
    memo: (component: unknown) => component, useMemo, useEffect, useLayoutEffect: useEffect,
    useCallback: (callback: unknown, deps: unknown[]) => useMemo(() => callback, deps),
    useRef: (value: unknown) => useMemo(() => ({ current: value }), []),
    useState(initial: unknown) {
      const runner = current, index = runner.cursor++
      if (!runner.slots[index]) {
        const slot = { value: typeof initial === 'function' ? initial() : initial, set: (_value: unknown) => {} }
        slot.set = value => { const next = typeof value === 'function' ? value(slot.value) : value; if (!Object.is(next, slot.value)) { slot.value = next; runner.dirty = true } }
        runner.slots[index] = slot
      }
      const slot = runner.slots[index]
      return [slot.value, slot.set]
    },
    useSyncExternalStore(subscribe: (listener: () => void) => () => void, snapshot: () => unknown) {
      const [value, setValue] = react.useState(snapshot)
      useEffect(() => { const update = () => setValue(snapshot()); update(); return subscribe(update) }, [subscribe, snapshot])
      return value
    },
  }
  const modules = new Map<string, { exports: any }>()
  function load(file: string): any {
    file = resolve(file)
    const cached = modules.get(file)
    if (cached) return cached.exports
    const module = { exports: {} as any }
    modules.set(file, module)
    if (file.endsWith('.css')) return module.exports
    const basename = file.split(/[\\/]/).at(-1)
    if (file.includes(`${resolve(root, 'src/components')}`) && !['RealFilmLibrary.tsx', 'RealFilmDetail.tsx', 'RealPlaybackHistory.tsx', 'useManualValue.ts', 'useFilmFavorite.ts'].includes(basename!)) {
      const noop = () => {}
      const specialized: Record<string, unknown> = { useDisplayPreferences: () => ({ value: { defaultFilmRatio: 'original', actorRatio: 'original', libraryRatios: {} }, loaded: true, busy: false, error: '' }), usePageScroll: () => noop, useButtonPress: noop, useMotionEnabled: () => false, getDrawPool: load(resolve(root, 'src/data/actorMetrics.ts')).getDrawPool }
      module.exports = new Proxy(specialized, { get(object, key: string) { return object[key] ??= () => null } })
      return module.exports
    }
    const requireLocal = (name: string) => {
      if (name === 'react') return react
      if (name === '@tauri-apps/api/event') return { listen: async (name: string, callback: (event: Event) => void) => {
        window.addEventListener(name, callback)
        return () => window.removeEventListener(name, callback)
      } }
      if (name === 'react/jsx-runtime') return { jsx: (type: unknown, props: unknown) => ({ type, props }), jsxs: (type: unknown, props: unknown) => ({ type, props }), Fragment: 'fragment' }
      if (name === '@tauri-apps/api/core') return { isTauri: () => true, invoke: async (command: string, args: any) => {
        commands.push(command)
        calls.push({ command, args })
        if (command === 'load_actor_catalog') return deferCatalog ? new Promise((resolve, reject) => pending.push({ resolve, reject })) : structuredClone(initialCatalog)
        if (command === 'load_launch_history') return deferHistory ? new Promise((resolve, reject) => pendingHistory.push({ resolve, reject })) : { films: structuredClone((initialCatalog as any).films), events: [], film_count: (initialCatalog as any).films.length, source_count: 1 }
        if (command === 'load_real_films') return structuredClone((initialCatalog as any).films)
        if (command === 'list_real_tags') return structuredClone((initialCatalog as any).tags)
        if (command === 'application_data_directory') return 'Z:/FICTIONAL_ONLY/software-data'
        if (command === 'load_interface_preferences') return load(resolve(root, 'src/data/settings.ts')).initialSettings
        return []
      } }
      if (name.startsWith('.')) {
        const target = resolve(dirname(file), name)
        return load(existsSync(target) ? target : existsSync(`${target}.ts`) ? `${target}.ts` : `${target}.tsx`)
      }
      return nodeRequire(name)
    }
    const compiled = ts.transpileModule(readFileSync(file, 'utf8'), { fileName: file, compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }).outputText
    runInNewContext(`(function(require, module, exports) { ${compiled}\n })`, { window, document, HTMLElement: class {}, sessionStorage: { getItem: () => null }, Event, CustomEvent, setTimeout, clearTimeout, queueMicrotask, URL, Blob })(requireLocal, module, module.exports)
    return module.exports
  }
  function runner(component: (props: any) => unknown): Runner {
    const value: Runner = { slots: [], cursor: 0, dirty: true, effects: [], output: null, render(props) {
      current = value; value.cursor = 0; value.dirty = false
      value.output = component(props)
      for (const commit of value.effects.splice(0)) commit()
    }, dispose() { for (const slot of value.slots) slot?.cleanup?.() } }
    return value
  }
  const Library = load(resolve(root, 'src/components/RealFilmLibrary.tsx')).RealFilmLibrary
  const Detail = load(resolve(root, 'src/components/RealFilmDetail.tsx')).RealFilmDetail
  const History = load(resolve(root, 'src/components/RealPlaybackHistory.tsx')).RealPlaybackHistory
  const ActorLibrary = load(resolve(root, 'src/components/ActorLibrary.tsx')).ActorLibrary
  const app = runner(load(resolve(root, 'src/App.tsx')).default)
  const library = runner(Library)
  const history = runner(History)
  const detail = runner(Detail)
  let detailOwner: 'app' | 'history' | null = null
  const find = (tree: any, predicate: (node: any) => boolean): any => {
    if (tree && predicate(tree)) return tree
    const children = Array.isArray(tree) ? tree : tree?.props?.children
    for (const child of Array.isArray(children) ? children : [children]) { const found = child && find(child, predicate); if (found) return found }
  }
  const text = (tree: any): string => typeof tree === 'string' || typeof tree === 'number' ? String(tree) : (Array.isArray(tree) ? tree : [tree?.props?.children]).map(child => child ? text(child) : '').join(' ')
  let libraryProps: any
  const render = () => {
    if (app.dirty) { app.render(); libraryProps = find(app.output, node => node.type === Library)?.props; if (libraryProps) library.dirty = true; if (detailOwner === 'app') detail.dirty = true }
    if (libraryProps && library.dirty) library.render(libraryProps)
    if (detailOwner === 'history' && history.dirty) { history.render({ active: true }); detail.dirty = true }
    if (detailOwner && detail.dirty) detail.render(find(detailOwner === 'app' ? app.output : history.output, node => node.type === Detail)?.props)
  }
  const flush = async () => { for (let index = 0; index < 20; index++) { render(); await new Promise(resolve => setImmediate(resolve)); if (!app.dirty && !library.dirty && (!detailOwner || !detail.dirty) && (detailOwner !== 'history' || !history.dirty)) return } throw new Error('Component effects did not settle') }
  return {
    commands, calls, pending, pendingHistory, flush, defer() { deferCatalog = true }, deferHistory() { deferHistory = true },
    change() { window.dispatchEvent(new Event('real-library-changed')) },
    manual: load(resolve(root, 'src/data/manualEvents.ts')).filmManualSaved,
    favorite: load(resolve(root, 'src/data/filmFavoriteEvents.ts')).favoriteSaved,
    readHistory: load(resolve(root, 'src/data/playback.ts')).loadLaunchHistory,
    props: () => libraryProps,
    text: () => text(library.output),
    drawer: () => find(library.output, node => typeof node.props?.onTags === 'function')?.props,
    openAppFilm(id: string) { detailOwner = 'app'; find(app.output, node => node.type === ActorLibrary).props.onOpenFilm(id); detail.dirty = true },
    mountHistory() { detailOwner = 'history'; history.dirty = true },
    openHistoryFilm(id: string) { find(history.output, node => typeof node.props?.onOpenFilm === 'function').props.onOpenFilm(id) },
    detailTag(name: string) { return find(detail.output, node => node.type === 'button' && node.props?.children === name && 'aria-pressed' in node.props)?.props },
    dispose() { detail.dispose(); history.dispose(); library.dispose(); app.dispose() },
  }
}
