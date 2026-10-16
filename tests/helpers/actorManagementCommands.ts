import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'
import * as state from '../../src/data/actorManagementState.ts'

export function actorManagementCommands() {
  const nodeRequire = createRequire(import.meta.url)
  const calls: { command: string; payload: any }[] = []
  const events: { type: string; payload?: any }[] = []
  const pending: { resolve: (value: unknown) => void; reject: (reason: unknown) => void }[] = []
  const module = { exports: {} as any }
  const source = readFileSync(new URL('../../src/data/actorManagement.ts', import.meta.url), 'utf8')
  const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
  const require = (name: string) => {
    if (name === './diagnostics.ts') return { invoke: (command: string, payload: unknown) => { calls.push({ command, payload }); return new Promise((resolve, reject) => pending.push({ resolve, reject })) } }
    if (name === './covers') return {
      invalidateChangedCovers: (ids: string[]) => events.push({ type: 'invalidate', payload: ids }),
      refreshMissingCovers: (ids: string[]) => events.push({ type: 'refresh-missing', payload: ids }),
    }
    if (name === './playback') return { notifyRealLibraryChanged: () => events.push({ type: 'refresh' }) }
    if (name === './realActors') return { actorManagedSaved: (id: string, input: unknown) => events.push({ type: 'saved', payload: { id, input } }) }
    if (name === './actorManagementState') return state
    return nodeRequire(name)
  }
  runInNewContext(`(function(require,module,exports){${compiled}\n})`)(require, module, module.exports)
  return { api: module.exports, calls, events, pending }
}
