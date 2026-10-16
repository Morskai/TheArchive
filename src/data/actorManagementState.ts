export type ManagedActorProfile = { birthMonth: string; height: string; cup: string; bust: string; waist: string; hips: string }
export type ActorDraft = ManagedActorProfile & { name: string; aliases: string }
export type ManagedActorInput = { actorId?: string; name: string; aliases: string[]; profile: ManagedActorProfile; photoPath: string | null; removePhoto: boolean; filmIds: string[] }
export type ActorFilmMatch = { folderName: string; videoFiles: string[]; filmId: string | null; existingActorIds: string[]; reason: string | null }
export type UnmatchedActorFilm = { actorName: string; folderName: string; videoFiles: string[]; reason: string }
export type ActorRelationships = { available: number; existing: number; notFound: number; unmatched: UnmatchedActorFilm[] }
export type ActorImportItem = { key: string; directory: string; name: string; aliases: string[]; candidateIds: string[]; defaultActorId?: string | null; canCreate: boolean; nameAliasSignature: string; status: 'new' | 'update' | 'conflict' | 'failed'; reason: string | null; profile?: Partial<ManagedActorProfile>; filmography?: ActorFilmMatch[] }
export type ActorImportPreview = { token: string; root: string; items: ActorImportItem[] }
export type ActorExportPreview = { token: string; directory: string; items: { actorId: string; name: string; directory: string; hasPhoto: boolean; exists: boolean; previousDirectories: string[] }[] }
export const emptyActorProfile: ManagedActorProfile = { birthMonth: '', height: '', cup: '', bust: '', waist: '', hips: '' }

type ActorFilmChoice = { title: string; code: string; library: string; sources?: { id: string; name: string }[] }
const filmSources = (film: ActorFilmChoice) => film.sources ?? (film.library ? [{ id: film.library, name: film.library }] : [])
export function actorEditorSources(films: ActorFilmChoice[]) {
  return [...new Map(films.flatMap(filmSources).map(source => [source.id, source])).values()].sort((a, b) => a.name.localeCompare(b.name, 'zh-CN') || a.id.localeCompare(b.id))
}
export function actorEditorFilms<T extends ActorFilmChoice>(films: T[], query: string, source: string): T[] {
  const term = query.trim().toLocaleLowerCase()
  return films.filter(film => (!term || `${film.title} ${film.code}`.toLocaleLowerCase().includes(term)) && (source === 'all' || filmSources(film).some(item => item.id === source)))
}
export const clearActorPersonalInfo = (draft: ActorDraft): ActorDraft => ({ ...draft, ...emptyActorProfile })
export type ActorScanResolution = { key: string; actorId: string | null; createNew: boolean }
export type ActorScanResult = { imported: number; updated: number; skipped: number; failed: number; conflicts: number; relationships: ActorRelationships; artwork_keys?: string[]; successful_artwork_keys?: string[] }

export function actorScanRelationships(items: ActorImportItem[], choices: ActorScanResolution[]): ActorRelationships {
  const result: ActorRelationships = { available: 0, existing: 0, notFound: 0, unmatched: [] }
  for (const item of items) {
    if (item.status === 'failed') continue
    const choice = actorScanChoice(item, choices)
    if ((item.status === 'conflict' && !choice) || (choice && !choice.createNew && !choice.actorId)) continue
    for (const film of item.filmography ?? []) {
      if (film.filmId) {
        if (choice?.actorId && film.existingActorIds.includes(choice.actorId)) result.existing++
        else result.available++
      } else {
        result.notFound++
        result.unmatched.push({ actorName: item.name, folderName: film.folderName, videoFiles: film.videoFiles, reason: film.reason ?? '未找到完全匹配的影片' })
      }
    }
  }
  return result
}

export function splitActorAliases(value: string) {
  return [...new Set(value.split(/[、,，\n]/).map(alias => alias.trim()).filter(Boolean))]
}
export function actorInput(draft: ActorDraft, filmIds: string[], actorId: string | undefined, photoPath: string | null, removePhoto: boolean): ManagedActorInput {
  const { birthMonth, height, cup, bust, waist, hips } = draft
  return { ...(actorId ? { actorId } : {}), name: draft.name.trim(), aliases: splitActorAliases(draft.aliases), profile: { birthMonth, height, cup: cup.toUpperCase(), bust, waist, hips }, photoPath, removePhoto, filmIds: [...new Set(filmIds)] }
}
export function actorEditorError(draft: ActorDraft, currentMonth: string) {
  if (!draft.name.trim() || [...draft.name.trim()].length > 200) return '姓名应为 1–200 个字符。'
  if (draft.aliases.length > 1000) return '别名过长，请控制在 1000 个字符内。'
  if (draft.birthMonth && (!/^\d{4}(-(0[1-9]|1[0-2]))?$/.test(draft.birthMonth) || draft.birthMonth < '1900' || draft.birthMonth > currentMonth)) return '出生信息请输入 YYYY 或 YYYY-MM，范围为 1900 年至本月。'
  const inRange = (value: string, min: number, max: number) => !value || (/^\d+$/.test(value) && Number(value) >= min && Number(value) <= max)
  if (!inRange(draft.height, 100, 230)) return '身高请输入 100–230 cm 的整数。'
  if ([draft.bust, draft.waist, draft.hips].some(value => !inRange(value, 40, 160))) return '三围请输入 40–160 cm 的整数。'
  if (draft.cup && !/^[a-z]$/i.test(draft.cup)) return '罩杯请输入一个英文字母。'
  return ''
}
export const pendingActorScanChoice = (items: ActorImportItem[], choices: ActorScanResolution[]) => items.find(item => item.status === 'conflict' && !choices.some(choice => choice.key === item.key))
export function actorScanChoice(item: ActorImportItem, choices: ActorScanResolution[]): ActorScanResolution | null {
  return choices.find(choice => choice.key === item.key) ?? (item.status === 'new'
    ? { key: item.key, actorId: null, createNew: true }
    : item.status === 'update' && item.defaultActorId ? { key: item.key, actorId: item.defaultActorId, createNew: false } : null)
}
export function actorScanBatchConflicts(items: ActorImportItem[], choices: ActorScanResolution[]) {
  const groups = new Map<string, ActorImportItem[]>()
  for (const item of items) {
    if (item.status === 'failed') continue
    const choice = actorScanChoice(item, choices)
    if (!choice || (!choice.createNew && !choice.actorId)) continue
    const target = choice.createNew ? `new:${item.nameAliasSignature}` : `actor:${choice.actorId}`
    groups.set(target, [...groups.get(target) ?? [], item])
  }
  return [...groups.values()].filter(group => group.length > 1)
}
export function chooseActorScanBatch(items: ActorImportItem[], choices: ActorScanResolution[], keys: string[], selectedKey: string | null) {
  const remaining = choices.filter(choice => !keys.includes(choice.key))
  return [...remaining, ...keys.map(key => {
    const item = items.find(item => item.key === key)
    return key === selectedKey && item ? actorScanChoice(item, choices) ?? { key, actorId: null, createNew: false } : { key, actorId: null, createNew: false }
  })]
}
export function actorManagementError(error: unknown): string {
  const code = String(error)
  const partial = /^actor_export_partial:(\d+):(.+)$/.exec(code)
  if (partial) return `已完成 ${partial[1]} 位演员导出，其余未完成。${actorManagementError(partial[2])}`
  return ({
    invalid_actor_profile: '个人资料格式或范围不正确，请检查后重试。', actor_invalid_name: '姓名或别名格式不正确，请检查后重试。', actor_invalid: '演员资料格式不正确，请检查姓名与个人资料。',
    actor_missing: '所选演员已变化，请关闭后重新打开。', actor_not_found: '所选演员已变化，请关闭后重新打开。', actor_save_pending: '该演员资料正在保存，请稍后再试。',
    actor_photo_invalid: '照片无法打开，请换一张 JPG、JPEG、PNG、WebP 或 AVIF 图片。', actor_photo_too_large: '照片过大，请选择较小的图片后重试。', actor_photo_unavailable: '照片暂时无法读取，请检查文件后重试。',
    actor_import_invalid: '演员资料格式不正确，请检查 actor 图片及 actor.info。', actor_name_conflict: '识别姓名与已有演员相同，请明确关联已有演员或创建新演员。', actor_import_selection_invalid: '所选更新与演员身份不一致，请重新预览并核对关联。',
    actor_preview_changed: '文件、演员资料或影片清单已变化，请重新选择目录并扫描。', actor_import_changed: '文件、演员资料或影片清单已变化，请重新选择目录并扫描。', actor_export_changed: '导出内容或演员资料已变化，请重新确认导出。',
    actor_unsafe_path: '目录含链接或不允许的路径，请选择普通文件夹后重试。', actor_import_unsafe: '导入目录含链接或无法安全读取的路径，请检查后重试。',
    actor_export_unsafe: '导出目录包含未知文件或链接，请检查导出目录后重试。', actor_export_exists: '导出内容已存在，请检查目录后重新确认导出。', actor_export_unavailable: '导出目录暂时无法写入，请检查权限后重试。', actor_export_failed: '演员导出未完成，请检查导出目录权限后重新预览。', actor_import_failed: '目录资料无法完整读取，请检查 actor 图片及 info 文件后重新预览。', actor_directory_unavailable: '演员资料目录暂时无法访问，请检查连接和权限后重试。', actor_save_failed: '演员资料未保存，请检查软件资料目录后重试。', info_invalid: 'actor.info 内容不正确，请检查演员资料文件后重新扫描。', info_too_large: 'actor.info 超过 1 MB，请缩小资料文件后重新扫描。', actor_transfer_moved: '演员资料导入与导出已移至演员库，请在那里操作。',
    actor_duplicate_identity: '姓名和别名与已有演员完全重复，请更新已有演员或调整资料。', actor_batch_conflict: '一位演员只能使用一份资料，请选一份后重试。',
    actor_export_film_unavailable: '所选演员有出演影片缺少当前视频清单。请连接对应片库并完成扫描后再导出，本次未写入导出文件。',
    actor_export_photo_in_use: '导出会覆盖或删除当前使用的演员照片，请选择其它导出位置。',
    protection_failed: '资料保护失败，本次演员资料未保存，请检查软件资料目录后重试。',
  } as Record<string, string>)[code] ?? '操作未完成，请检查文件与目录权限后重试。'
}
