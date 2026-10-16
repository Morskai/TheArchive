import { invoke } from './diagnostics.ts'

export interface LibraryProblem {
  id: string
  kind: 'film_missing_cover' | 'film_code_conflict'
  subject_id: string
  title: string
  reason: string
  videos: { id: string; name: string }[]
  directories?: string[]
  directory_options?: string[]
  directory_targets?: { source_id: string; relative: string; label: string }[]
  expected_file: string
  sources: { id: string; name: string }[]
  handled: boolean
}
export type ProblemStatus = 'open' | 'resolved'
export type ProblemKindFilter = 'all' | LibraryProblem['kind']

export interface ProblemGroup {
  key: string
  title: string
  directory: string
  sourceId: string
  items: { problem: LibraryProblem; directoryIndex: number }[]
}

export function selectedProblemGroups(selectedKeys: string[], visible: ProblemGroup[]) {
  const selected = new Set(selectedKeys)
  return visible.filter(group => selected.has(group.key))
}

export function problemGroupIssueIds(groups: ProblemGroup[]) {
  return [...new Set(groups.flatMap(group => group.items.map(item => item.problem.id)))]
}

export function toggleAllProblemGroups(selectedKeys: string[], visible: ProblemGroup[]) {
  const selected = new Set(selectedKeys)
  return visible.every(group => selected.has(group.key)) ? [] : visible.map(group => group.key)
}

export function groupDirectoryProblems(problems: LibraryProblem[]): ProblemGroup[] {
  const groups = new Map<string, ProblemGroup>()
  for (const problem of problems) {
    const targets = problem.directory_targets?.length ? problem.directory_targets :
      problem.sources.length === 1 ? (problem.directory_options ?? []).map(label => ({ source_id: problem.sources[0].id, relative: label, label })) : []
    const choices = targets.length ? targets : [{ source_id: '', relative: '', label: '' }]
    choices.forEach((target, directoryIndex) => {
      const path = target.relative.replace(/\\/g, '/').replace(/\/+$/, '')
      const key = targets.length ? JSON.stringify([target.source_id, path.toLocaleLowerCase()]) : `unknown:${problem.id}`
      let group = groups.get(key)
      if (!group) {
        group = { key, title: target.label.split(/[\\/]/).pop() || problem.title, directory: target.label, sourceId: target.source_id, items: [] }
        groups.set(key, group)
      }
      if (!group.items.some(item => item.problem.id === problem.id)) group.items.push({ problem, directoryIndex })
    })
  }
  return [...groups.values()]
}

export function filterProblemGroups(groups: ProblemGroup[], status: ProblemStatus, source: string, query: string, kind: ProblemKindFilter = 'all', softwareText?: (problem: LibraryProblem) => string[]) {
  const needle = query.trim().toLocaleLowerCase('zh-CN')
  return groups.flatMap(group => {
    if (source !== 'all' && group.sourceId && group.sourceId !== source) return []
    const directoryMatch = !!needle && [group.title, group.directory].some(value => value.toLocaleLowerCase('zh-CN').includes(needle))
    const items = group.items.filter(item => (kind === 'all' || item.problem.kind === kind) && filterLibraryProblems([item.problem], status, group.sourceId ? 'all' : source, '').length &&
      (!needle || directoryMatch || [item.problem.title, problemLabel(item.problem), item.problem.reason, item.problem.expected_file, ...item.problem.videos.map(video => video.name), ...softwareText?.(item.problem) ?? []].some(value => value.toLocaleLowerCase('zh-CN').includes(needle))))
    return items.length ? [{ ...group, items }] : []
  })
}
export const loadLibraryProblems = () => invoke<LibraryProblem[]>('load_library_problems')
export const restoreLibraryProblems = (issueIds: string[]) => invoke<void>('restore_library_problems', { issueIds })
export const clearLibraryProblems = (issueIds: string[]) => invoke<void>('clear_library_problems', { issueIds })
export const markLibraryProblems = (issueIds: string[]) => invoke<void>('mark_library_problems', { issueIds })
export function problemDirectoryRequest(problem: LibraryProblem, directoryIndex: number) {
  if (!Number.isInteger(directoryIndex) || directoryIndex < 0 || directoryIndex >= (problem.directory_options?.length ?? 0)) {
    throw new Error('problem_directory_unavailable')
  }
  return { problemId: problem.id, directoryIndex }
}
export const openProblemDirectory = (problem: LibraryProblem, directoryIndex: number) => invoke<void>('open_problem_directory', problemDirectoryRequest(problem, directoryIndex))
export const problemLabel = (problem: Pick<LibraryProblem, 'kind'>) => ({ film_missing_cover: '封面缺失', film_code_conflict: '编号重复' })[problem.kind]

export function filterLibraryProblems(problems: LibraryProblem[], status: ProblemStatus, source: string, query: string, softwareText?: (problem: LibraryProblem) => string[]) {
  const needle = query.trim().toLocaleLowerCase('zh-CN')
  return problems.filter(problem => {
    if (status === 'open' && problem.handled || status === 'resolved' && !problem.handled) return false
    if (source !== 'all' && !problem.sources.some(item => item.id === source)) return false
    return !needle || [problem.title, problemLabel(problem), problem.reason, problem.expected_file, ...problem.videos.map(item => item.name), ...problem.directories ?? [], ...problem.directory_options ?? [], ...softwareText?.(problem) ?? []]
      .some(value => value.toLocaleLowerCase('zh-CN').includes(needle))
  })
}

export function problemDirectoryError(error: unknown) {
  switch (error instanceof Error ? error.message : String(error)) {
    case 'problem_not_found': return '此问题已更新，请重新读取。'
    case 'source_paused': return '片库已停用，请先在设置中启用。'
    case 'source_removed': return '片库已移除，请先在设置中重新添加。'
    case 'root_unavailable': return '片库目录无法访问，请检查磁盘连接。'
    case 'problem_directory_unavailable':
    case 'directory_unavailable': return '此目录暂不可用，请重新读取或检查磁盘。'
    case 'path_outside_source': return '此目录不在当前片库范围内，无法打开。'
    case 'access_denied': return 'Windows 拒绝了打开请求，请检查权限。'
    default: return '目录未能打开，请稍后重试。'
  }
}

export function visibleProblemSelection(selected: string[], visible: LibraryProblem[], status: ProblemStatus = 'open') {
  const available = new Set(visible.filter(problem => problem.handled === (status === 'resolved')).map(problem => problem.id))
  return [...new Set(selected)].filter(id => available.has(id))
}

export function problemError(error: unknown) {
  switch (String(error)) {
    case 'problem_not_found': return '此问题暂不可用，请重新读取。'
    case 'problem_save_failed': return '处理状态未保存，请重试。'
    default: return '问题资料暂无法读取或保存，请重试。'
  }
}
