import { message, type Message, type UiText } from './core.ts'
import { actorManagementError } from '../data/actorManagementState.ts'
import type { SourceConfirmation, SourcePreview } from '../data/sources.ts'

export function actorManagementMessage(error: unknown): Message {
  const partial = /^actor_export_partial:(\d+):(.+)$/.exec(String(error))
  return partial ? message('已完成 {0} 位演员导出，其余未完成。{1}', [Number(partial[1]), message(actorManagementError(partial[2]))]) : message(actorManagementError(error))
}
export function sourceRefreshMessage(result: SourceConfirmation): Message | '' {
  const parts = [result.related_retained ? message('相关目录未完成核对，保留原资料') : '', result.recovery_failed ? message('部分影片资料关联未完成，下次扫描会重试') : '', result.artwork_failed ? message('部分图片缓存未生成，可重新扫描') : ''].filter(Boolean)
  if (!parts.length) return ''
  return message('；{0}。', [message(parts.map((_, i) => `{${i}}`).join('；'), parts)])
}
export const sourcePreviewMessage = (preview: SourcePreview) => message('发现 {0} 个视频 · 待确认添加', [preview.video_count])
export const sourceAddedMessage = (preview: SourcePreview, result: SourceConfirmation) => message('目录已添加 · {0} 个视频{1}', [preview.video_count, sourceRefreshMessage(result)])

export function problemReasonMessage(reason: string): UiText {
  const prefix = '扫描证据已变化，需要重新核对。'
  return reason.startsWith(prefix) ? message('扫描证据已变化，需要重新核对。{0}', [message(reason.slice(prefix.length))]) : reason
}

export function ignoredNameMessage(error: unknown): UiText {
  const raw = error instanceof Error ? error.message : String(error)
  const duplicate = /^目录名称“(.*)”重复，请合并条目。$/.exec(raw)
  return duplicate ? message('目录名称“{0}”重复，请合并条目。', [duplicate[1]]) : raw
}
