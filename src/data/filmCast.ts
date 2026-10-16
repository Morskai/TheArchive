export interface FilmCastActor { id: string; name: string; system: boolean }
export interface FilmCastCandidate { id: string; name: string; library: string }

export function filterCastCandidates(candidates: FilmCastCandidate[], cast: FilmCastActor[], query: string) {
  const selected = new Set(cast.map(actor => actor.id))
  const term = query.trim().toLocaleLowerCase()
  return candidates.filter(actor => !selected.has(actor.id) && (!term || actor.name.toLocaleLowerCase().includes(term)))
}

export function castSaveMessage(error: unknown) {
  const code = String(error)
  if (code === 'protection_failed') return '资料保护备份失败，出演表未更新，请检查应用数据目录后重试。'
  if (code === 'cast_candidate_unavailable' || code === 'source_paused' || code === 'source_removed') return '演员来源已停用或移除，请重新打开出演表后选择。'
  if (code === 'cast_save_pending') return '此影片的出演表正在保存，请稍后重试。'
  if (code === 'film_not_found' || code === 'film_inactive') return '影片来源已停用或移除，请关闭详情并检查片库状态。'
  return '出演表未保存，请重试。'
}
