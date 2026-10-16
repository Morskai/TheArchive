import { invoke } from './diagnostics.ts'
import { notifyRealLibraryChanged } from './playback'

export const loadActorTags = () => invoke<string[]>('list_actor_tags')
async function save(command: string, args: Record<string, unknown>) {
  await invoke<void>(command, args)
  notifyRealLibraryChanged()
}
export const createActorTag = (name: string) => save('create_actor_tag', { name })
export const renameActorTag = (name: string, newName: string) => save('rename_actor_tag', { name, newName })
export const deleteActorTag = (name: string) => save('delete_actor_tag', { name })
export const reorderActorTags = (names: string[]) => save('reorder_actor_tags', { names })
export async function applyActorBulkTag(actorIds: string[], name: string, add: boolean) {
  const count = await invoke<number>('apply_actor_bulk_tag', { actorIds: [...new Set(actorIds)], name, add })
  notifyRealLibraryChanged()
  return count
}

export function actorTagError(error: unknown): string {
  const code = String(error)
  if (code === 'protection_failed') return '资料保护失败，演员标签未删除，请检查软件资料目录后重试。'
  if (code === 'duplicate_tag') return '这个标签已经存在。'
  if (code === 'invalid_tag') return '标签名称应为 1 至 20 个字符。'
  if (code === 'tag_limit') return '最多可创建 30 个演员标签。'
  if (code === 'actor_not_found') return '所选演员已发生变化，请刷新后重新选择。'
  return '演员标签未保存，请重试。'
}
