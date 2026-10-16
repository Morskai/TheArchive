import { isTauri } from '@tauri-apps/api/core'
import { invoke } from './diagnostics.ts'

export interface LibrarySource {
  id: string
  name: string
  root_path: string
  observation_count: number
  film_count: number
  video_bytes?: number | null
  last_scanned_at?: string | null
  enabled: boolean
  removed: boolean
}

export interface SourcePreview {
  token: string | null
  video_count: number
  needs_review: number
  issue_count: number
  unreadable_directory_count: number
  complete: boolean
  video_directory_count: number
  image_count: number
  subtitle_count: number
  cover_hint_count: number
}

export interface SourceConfirmation {
  library_id: string
  discovered: number
  inserted: number
  missing_count: number
  related_retained?: number
  recovery_failed?: boolean
  artwork_failed?: number
  artwork_keys?: string[]
  successful_artwork_keys?: string[]
}

export function sourceRefreshNotice(result: SourceConfirmation) {
  const parts: string[] = []
  if (result.related_retained) parts.push('相关目录未完成核对，保留原资料')
  if (result.recovery_failed) parts.push('部分影片资料关联未完成，下次扫描会重试')
  if (result.artwork_failed) parts.push('部分图片缓存未生成，可重新扫描。')
  return parts.length ? `；${parts.join('；')}` : ''
}

export function sourcePreviewNotice(preview: SourcePreview) {
  return `发现 ${preview.video_count} 个视频 · 待确认添加`
}

export function sourceAddedNotice(preview: SourcePreview, confirmation: SourceConfirmation) {
  return `目录已添加 · ${preview.video_count} 个视频${sourceRefreshNotice(confirmation)}`
}

export const nativeSourcesAvailable = isTauri()

export const listLibrarySources = () => invoke<LibrarySource[]>('list_library_sources')
export const openLibrarySourceDirectory = (libraryId: string) => invoke<void>('open_library_source_directory', { libraryId })
export const chooseLibraryRoot = () => invoke<string | null>('choose_library_root')
export const previewLibrarySource = (root: string, libraryId: string | null) =>
  invoke<SourcePreview>('preview_library_source', { root, libraryId })
export const changeLibrarySourceState = (libraryId: string, action: 'pause' | 'resume' | 'remove') =>
  invoke<void>('change_library_source_state', { libraryId, action })
export const cancelLibraryPreview = (token: string) =>
  invoke<void>('cancel_library_preview', { token })
export const confirmLibrarySource = (token: string) =>
  invoke<SourceConfirmation>('confirm_library_source', { token })
