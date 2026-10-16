export interface DiagnosticEvent {
  origin: 'frontend'
  operation: string
  event: string
  operationId?: string
  durationMs?: number
  errorCode?: string
  source?: string
  line?: number
  column?: number
  counts?: Record<string, number>
}

const tasks = new Set(['initialize_library_session', 'preview_library_source', 'confirm_library_source',
  'change_library_source_state', 'preview_actor_import', 'confirm_actor_scan_add', 'preview_actor_export',
  'confirm_actor_export', 'save_managed_actor', 'create_data_backup', 'restore_data_backup', 'clear_user_data',
  'clear_generated_cache', 'start_title_translation', 'test_translation_connection', 'clear_translation_memory'])
const countFields: Record<string, string[]> = {
  initialize_library_session: ['savedSources'],
  preview_library_source: ['video_count', 'issue_count', 'unreadable_directory_count', 'video_directory_count', 'image_count', 'subtitle_count'],
  confirm_library_source: ['discovered', 'inserted', 'missing_count', 'related_retained', 'artwork_failed'],
  confirm_actor_scan_add: ['imported', 'updated', 'skipped', 'failed', 'conflicts'],
  confirm_actor_export: ['exported'],
  start_title_translation: ['total', 'completed', 'succeeded', 'failed', 'reused', 'api_requests'],
}
const knownErrors = new Set(['invalid_interface_language', 'interface_language_load_failed', 'interface_language_save_failed','translation_auth', 'translation_quota', 'translation_rate_limit', 'translation_timeout',
  'translation_network', 'translation_server', 'translation_invalid_response', 'translation_invalid_key',
  'translation_key_missing', 'translation_credentials_error', 'translation_credentials_unsupported',
  'translation_cancelled', 'translation_busy', 'translation_failed', 'protection_failed', 'storage_unavailable',
  'catalog_failed', 'startup_failed', 'scan_failed', 'scan_changed', 'incomplete', 'root_unavailable',
  'source_changed', 'preview_expired', 'import_failed', 'state_failed', 'actor_import_failed', 'actor_export_failed', 'actor_export_photo_in_use',
  'actor_photo_unreadable', 'actor_save_failed', 'actor_delete_failed', 'reset_failed', 'reset_credentials_failed',
  'reset_cleanup_pending', 'reset_recovery_required', 'cache_clear_failed', 'launch_failed', 'launch_result_unknown',
  'cover_load_failed', 'cover_unavailable', 'cover_changed', 'file_unavailable', 'actors_load_failed', 'problems_load_failed'])
export function taskCounts(command: string, result: unknown): Record<string, number> {
  const counts: Record<string, number> = {}
  if (!tasks.has(command) || !result || typeof result !== 'object') return counts
  const object = result as Record<string, unknown>
  const number = (field: string, value: unknown) => { if (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0) counts[field] = value }
  for (const field of countFields[command] ?? []) number(field, object[field])
  if (command === 'preview_actor_import' || command === 'preview_actor_export') { if (Array.isArray(object.items)) counts.items = object.items.length }
  if (command === 'confirm_actor_scan_add' && object.relationships && typeof object.relationships === 'object') {
    for (const field of ['available', 'existing', 'not_found']) number(field, (object.relationships as Record<string, unknown>)[field])
  }
  return counts
}

type RawInvoke = <T = unknown>(command: string, args?: Record<string, unknown>) => Promise<T>
type SendEvent = (event: DiagnosticEvent) => unknown
function safelySend(send: SendEvent, event: DiagnosticEvent) {
  try { void Promise.resolve(send(event)).catch(() => {}) } catch { /* Diagnostics cannot replace business results. */ }
}
let serial = 0
export function createDiagnosticInvoke(raw: RawInvoke, send: SendEvent, time: () => number = () => globalThis.performance?.now() ?? Date.now()): RawInvoke {
  return async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
    const id = `op-${Date.now().toString(36)}-${(++serial).toString(36)}`
    const started = time()
    const base = { origin: 'frontend' as const, operation: command, operationId: id }
    const task = tasks.has(command)
    if (task) safelySend(send, { ...base, event: 'start' })
    try {
      const correlated = task || command === 'load_real_films' || command === 'load_actor_catalog'
      const result = await raw<T>(command, correlated ? { ...args, diagnosticOperationId: id } : args)
      if (task) {
        let counts = {}
        try { counts = taskCounts(command, result) } catch { /* A diagnostic summary must not change the result. */ }
        safelySend(send, { ...base, event: 'complete', durationMs: Math.max(0, Math.round(time() - started)), counts })
      }
      return result
    } catch (error) {
      // Rust validates a fixed error-code allowlist. No arbitrary exception or request serialization.
      safelySend(send, { ...base, event: 'failed', durationMs: Math.max(0, Math.round(time() - started)), errorCode: typeof error === 'string' && knownErrors.has(error) ? error : 'unknown_error' })
      throw error
    }
  }
}

export function exceptionEvent(event: 'error' | 'unhandled_rejection' | 'render_error' | 'resource_error', error: unknown, line?: number, column?: number): DiagnosticEvent {
  const known: Record<string, string> = { TypeError: 'javascript_type_error', ReferenceError: 'javascript_reference_error', RangeError: 'javascript_range_error', SyntaxError: 'javascript_syntax_error' }
  const record: DiagnosticEvent = { origin: 'frontend', operation: 'frontend_exception', event, errorCode: event === 'resource_error' ? 'resource_load_error' : 'javascript_error' }
  if (error instanceof Error) {
    record.errorCode = known[error.name] ?? 'javascript_error'
    // Keep only compiled bundle line/column; the message, URL, stack and local paths stay private.
    const location = error.stack?.match(/\/assets\/[a-zA-Z0-9_-]+\.js:(\d+):(\d+)/)
    if (location) { record.source = 'frontend_bundle'; line ??= Number(location[1]); column ??= Number(location[2]) }
  }
  if (Number.isSafeInteger(line) && line! > 0 && line! <= 0xffffffff) record.line = line
  if (Number.isSafeInteger(column) && column! > 0 && column! <= 0xffffffff) record.column = column
  return record
}
