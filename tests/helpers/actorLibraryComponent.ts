import { readFileSync, existsSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'
import type { Actor } from '../../src/data/actors'

// Run the real ActorLibrary body/effects. Native IPC and untouched child surfaces
// are boundaries; selection, filter handlers and destructive-action guards remain real.
export function actorLibraryComponent(actors: Actor[], initialTags = ['精选', '已看']) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
  const nodeRequire = createRequire(import.meta.url)
  const commands: { command: string; args: any }[] = []
  const confirms: string[] = []
  let confirmResult = true
  let failure = ''
  let tagNames = [...initialTags]
  const listeners = new Map<string, Set<(event: Event) => void>>()
  const window = {
    confirm: (message: string) => { confirms.push(message); return confirmResult },
    addEventListener(name: string, callback: (event: Event) => void) { const current = listeners.get(name) ?? new Set(); current.add(callback); listeners.set(name, current) },
    removeEventListener(name: string, callback: (event: Event) => void) { listeners.get(name)?.delete(callback) },
    dispatchEvent(event: Event) { for (const callback of listeners.get(event.type) ?? []) callback(event) },
  }
  const document = { querySelector: () => null }
  const slots: any[] = []
  let cursor = 0, dirty = true
  const effects: (() => void)[] = []
  const equal = (a: unknown[] | undefined, b: unknown[] | undefined) => a && b && a.length === b.length && a.every((value, index) => Object.is(value, b[index]))
  const useMemo = (callback: () => unknown, deps: unknown[]) => {
    const index = cursor++, old = slots[index]
    if (!old || !equal(old.deps, deps)) slots[index] = { value: callback(), deps }
    return slots[index].value
  }
  const useEffect = (callback: () => void | (() => void), deps?: unknown[]) => {
    const index = cursor++, old = slots[index]
    if (old && equal(old.deps, deps)) return
    const slot = { deps, cleanup: old?.cleanup }; slots[index] = slot
    effects.push(() => { slot.cleanup?.(); slot.cleanup = callback() })
  }
  const react = {
    useSyncExternalStore(subscribe: (listener: () => void) => () => void, snapshot: () => unknown) {
      const [value, setValue] = react.useState(snapshot)
      useEffect(() => { const update = () => setValue(snapshot()); update(); return subscribe(update) }, [subscribe, snapshot])
      return value
    },
    memo: (component: unknown) => component, useMemo, useEffect, useLayoutEffect: useEffect,
    useCallback: (callback: unknown, deps: unknown[]) => useMemo(() => callback, deps),
    useRef: (value: unknown) => useMemo(() => ({ current: value }), []),
    useState(initial: unknown) {
      const index = cursor++
      if (!slots[index]) {
        const slot = { value: typeof initial === 'function' ? initial() : initial, set: (_value: unknown) => {} }
        slot.set = value => { const next = typeof value === 'function' ? value(slot.value) : value; if (!Object.is(next, slot.value)) { slot.value = next; dirty = true } }
        slots[index] = slot
      }
      return [slots[index].value, slots[index].set]
    },
  }
  const modules = new Map<string, { exports: any }>()
  function load(file: string): any {
    file = resolve(file)
    const cached = modules.get(file); if (cached) return cached.exports
    const module = { exports: {} as any }; modules.set(file, module)
    if (file.endsWith('.css')) return module.exports
    if (file.startsWith(resolve(root, 'src/components')) && !file.endsWith('ActorLibrary.tsx')) {
      module.exports = new Proxy({}, { get(object: any, key: string) { return object[key] ??= function Child() {} } }); return module.exports
    }
    const requireLocal = (name: string) => {
      if (name === 'react') return react
      if (name === 'react/jsx-runtime') return { jsx: (type: unknown, props: unknown) => ({ type, props }), jsxs: (type: unknown, props: unknown) => ({ type, props }), Fragment: 'fragment' }
      if (name === '@tauri-apps/api/core') return { isTauri: () => true, invoke: async (command: string, args: any) => {
        commands.push({ command, args })
        if (command === failure) { failure = ''; throw 'protection_failed' }
        if (command === 'list_actor_tags') return [...tagNames]
        if (command === 'create_actor_tag') tagNames.push(args.name)
        if (command === 'rename_actor_tag') tagNames = tagNames.map(tag => tag === args.name ? args.newName : tag)
        if (command === 'delete_actor_tag') tagNames = tagNames.filter(tag => tag !== args.name)
        if (command === 'reorder_actor_tags') tagNames = [...args.names]
        if (command === 'delete_actors' || command === 'apply_actor_bulk_tag') return new Set(args.actorIds).size
        return null
      } }
      if (name.startsWith('.')) { const target = resolve(dirname(file), name); return load(existsSync(target) ? target : existsSync(`${target}.ts`) ? `${target}.ts` : `${target}.tsx`) }
      return nodeRequire(name)
    }
    const compiled = ts.transpileModule(readFileSync(file, 'utf8'), { fileName: file, compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX } }).outputText
    runInNewContext(`(function(require, module, exports) { ${compiled}\n })`, { window, document, Event, CustomEvent, URL, Blob, setTimeout, clearTimeout, requestAnimationFrame: () => 1, cancelAnimationFrame() {} })(requireLocal, module, module.exports)
    return module.exports
  }
  const Component = load(resolve(root, 'src/components/ActorLibrary.tsx')).ActorLibrary
  const detailChanges: boolean[] = []
  let props: any = { actors, films: [], query: '', detailOpen: false, initialDetailId: null, onClearQuery: () => { props.query = ''; dirty = true }, onDetailViewChange: (open: boolean) => { detailChanges.push(open); props.detailOpen = open; dirty = true }, onDetailBack() {}, onUpdateActor: async (id: string, patch: Partial<Actor>) => { props.actors = props.actors.map((actor: Actor) => actor.id === id ? { ...actor, ...patch } : actor); dirty = true }, onOpenFilm() {} }
  props.missingPhotosOnly = false
  props.onClearPhotoFilter = () => { props.missingPhotosOnly = false; dirty = true }
  let output: any
  const walk = (tree: any, predicate: (node: any) => boolean): any[] => {
    if (!tree) return []
    const children = Array.isArray(tree) ? tree : tree?.props?.children
    return [...(predicate(tree) ? [tree] : []), ...(Array.isArray(children) ? children : [children]).flatMap(child => child ? walk(child, predicate) : [])]
  }
  const text = (tree: any): string => typeof tree === 'string' || typeof tree === 'number' ? String(tree) : (Array.isArray(tree) ? tree : [tree?.props?.children]).map(child => child !== null && child !== undefined && child !== false ? text(child) : '').join(' ')
  return {
    commands, confirms, detailChanges,
    async flush() { for (let index = 0; index < 30; index++) { if (dirty) { cursor = 0; dirty = false; output = Component(props); for (const commit of effects.splice(0)) commit() } await new Promise(resolve => setImmediate(resolve)); if (!dirty) return } throw new Error('Effects did not settle') },
    update(patch: any) { props = { ...props, ...patch }; dirty = true },
    confirm(value: boolean) { confirmResult = value },
    fail(command: string) { failure = command },
    nodes: (predicate: (node: any) => boolean) => walk(output, predicate),
    button(label: string) { const item = walk(output, node => node.type === 'button' && (node.props['aria-label'] === label || text(node).trim() === label))[0]; assertFound(item, label); return item.props },
    control(label: string) { const item = walk(output, node => node.props?.['aria-label'] === label)[0]; assertFound(item, label); return item.props },
    text: () => text(output),
    dispose() { for (const slot of slots) slot?.cleanup?.() },
  }
}

function assertFound(item: unknown, label: string) { if (!item) throw new Error(`Missing control: ${label}`) }
