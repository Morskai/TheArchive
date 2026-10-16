import { isTauri } from '@tauri-apps/api/core'
import { invoke } from './diagnostics.ts'

export interface BackupInfo {
  id: string
  created_at_unix_ms: number
}

export const nativeBackupsAvailable = isTauri()

export function listDataBackups(): Promise<BackupInfo[]> {
  return invoke<BackupInfo[]>('list_data_backups')
}

export function createDataBackup(): Promise<string> {
  return invoke<string>('create_data_backup')
}

export function restoreDataBackup(backupId: string): Promise<void> {
  return invoke<void>('restore_data_backup', { backupId })
}

export function deleteDataBackup(backupId: string): Promise<void> {
  return invoke<void>('delete_data_backup', { backupId })
}
