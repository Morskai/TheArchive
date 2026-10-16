import { invoke } from './diagnostics.ts'

export interface LibrarySessionResult {
  savedSources: number
}

type ResetNotice = 'complete' | 'cleanup_pending'
const resetNoticeKey = 'film-library-reset-notice'
export function rememberResetNotice(notice: ResetNotice) {
  try { sessionStorage.setItem(resetNoticeKey, notice) } catch { /* Current feedback still displays. */ }
}
export function readResetNotice(): ResetNotice | null {
  try {
    const notice = sessionStorage.getItem(resetNoticeKey)
    return notice === 'complete' || notice === 'cleanup_pending' ? notice : null
  } catch { return null }
}
export function dismissResetNotice() {
  try { sessionStorage.removeItem(resetNoticeKey) } catch { /* No saved usage data is involved. */ }
}

// Shared across React StrictMode effects. The desktop process also guards this
// saved-data read, so a rejected IPC request can be retried safely.
export function createLibraryLifecycle(command: (name: string) => Promise<unknown>) {
  let initialization: Promise<LibrarySessionResult> | undefined
  return {
    reset() { initialization = undefined },
    initialize() {
      if (!initialization) {
        initialization = command('initialize_library_session').then(result => result as LibrarySessionResult)
          .catch(error => { initialization = undefined; throw error })
      }
      return initialization
    },
  }
}

const lifecycle = createLibraryLifecycle(name => invoke(name))
export const initializeLibrarySession = () => lifecycle.initialize()
export const resetLibrarySession = () => lifecycle.reset()
export const clearGeneratedCache = () => invoke<void>('clear_generated_cache')
export async function clearUserData() {
  try {
    await invoke<void>('clear_user_data')
    lifecycle.reset()
  } catch (error) {
    if (String(error) === 'reset_cleanup_pending') lifecycle.reset()
    throw error
  }
}

export function maintenanceError(error: unknown) {
  switch (String(error)) {
    case 'protection_failed': return '无法保护当前软件资料，操作已停止。'
    case 'cache_clear_failed': return '图片缓存未能清空，请稍后重试。'
    case 'reset_failed':
    case 'user_data_clear_failed': return '未能恢复初始状态，原有资料已保留，请稍后重试。'
    case 'reset_credentials_failed': return 'API 凭据未能安全清理，原有资料已保留，请检查 Windows 凭据存储后重试。'
    case 'reset_cleanup_pending': return '资料已恢复初始状态，临时保护清理尚未完成；请重启软件继续处理。'
    case 'reset_recovery_required': return '保护资料尚未完成恢复，已暂停资料读写。请重启软件继续恢复。'
    default: return '操作未完成，请稍后重试。'
  }
}
