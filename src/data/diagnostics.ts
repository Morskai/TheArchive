import { invoke as rawInvoke, isTauri } from '@tauri-apps/api/core'
import { createDiagnosticInvoke, exceptionEvent, type DiagnosticEvent } from './diagnosticEvents.ts'

// Bounded before crossing IPC, as well as in Rust. Image errors and repeated exceptions cannot flood it.
const repeats = new Map<string, number>()
let pending = 0
export function reportDiagnostic(event: DiagnosticEvent) {
  if (!isTauri() || pending >= 32) return
  if (event.errorCode && (event.operation === 'frontend_exception' || ['load_actor_thumbnail', 'load_film_actor_thumbnail', 'load_cover_thumbnail', 'load_version_cover_thumbnail'].includes(event.operation))) {
    const key = `${event.operation}:${event.event}:${event.errorCode}`
    if (!repeats.has(key) && repeats.size >= 128) return
    const count = (repeats.get(key) ?? 0) + 1
    repeats.set(key, count)
    if (count > 5) return
  }
  pending++
  try {
    void rawInvoke('write_diagnostic_event', { event }).catch(() => {}).finally(() => { pending-- })
  } catch { pending-- }
}
export const invoke = createDiagnosticInvoke(rawInvoke, reportDiagnostic)

export function installFrontendDiagnostics() {
  if (!isTauri()) return () => {}
  const onError = (event: Event) => {
    if (event instanceof ErrorEvent) reportDiagnostic(exceptionEvent('error', event.error, event.lineno, event.colno))
    else reportDiagnostic(exceptionEvent('resource_error', undefined))
  }
  const onRejection = (event: PromiseRejectionEvent) => reportDiagnostic(exceptionEvent('unhandled_rejection', event.reason))
  window.addEventListener('error', onError, true)
  window.addEventListener('unhandledrejection', onRejection)
  return () => { window.removeEventListener('error', onError, true); window.removeEventListener('unhandledrejection', onRejection) }
}

export interface DiagnosticStatus { available: boolean; maxFileBytes: number; maxFiles: number }
export const loadDiagnosticStatus = () => rawInvoke<DiagnosticStatus>('diagnostic_log_status')
export const openDiagnosticDirectory = () => rawInvoke<void>('open_diagnostic_directory')
export const exportDiagnosticLogs = () => rawInvoke<string | null>('export_diagnostic_logs')
