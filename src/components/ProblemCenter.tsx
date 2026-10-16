import { problemReasonMessage } from '../i18n/feedback'
import { t, message as uiMessage, useLocale, type UiText } from '../i18n/react'
import { AppSelect } from './AppSelect'
import { ListPagination } from './ListPagination'
import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { realFilmsAvailable } from '../data/realFilms'
import { groupDirectoryProblems, filterProblemGroups, selectedProblemGroups, problemGroupIssueIds, toggleAllProblemGroups, loadLibraryProblems, markLibraryProblems, restoreLibraryProblems, clearLibraryProblems, openProblemDirectory, problemDirectoryError, problemLabel, problemError, visibleProblemSelection, type LibraryProblem, type ProblemStatus, type ProblemKindFilter } from '../data/problems'
import { isProblemSelectionSurface } from '../data/problemSelection'
import '../problem-center.css'

const pageSize = 20

function ProblemDirectoryAction({ problem, directoryIndex, title }: { problem: LibraryProblem; directoryIndex: number; title: string }) {
  useLocale()
  const options = problem.directory_options ?? []
  const [phase, setPhase] = useState<'pending' | 'success' | null>(null)
  const [error, setError] = useState('')
  const guard = useRef(false)
  const mounted = useRef(false)
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  useEffect(() => {
    if (phase !== 'success') return
    const timer = window.setTimeout(() => setPhase(null), 1700)
    return () => window.clearTimeout(timer)
  }, [phase])
  const openDirectory = async () => {
    if (!realFilmsAvailable || guard.current || !options.length) return
    guard.current = true; setPhase('pending'); setError('')
    try { await openProblemDirectory(problem, directoryIndex); if (mounted.current) setPhase('success') }
    catch (reason) { if (mounted.current) { setPhase(null); setError(problemDirectoryError(reason)) } }
    finally { guard.current = false }
  }
  return <div className="issue-directory-action">
    <div className="issue-directory-controls">
      <button type="button" className={phase === 'success' ? 'action-succeeded' : ''} aria-busy={phase === 'pending'} aria-label={phase === 'success' ? t("打开{0}目录，请求成功", [title]) : t("打开{0}目录", [title])} disabled={!realFilmsAvailable || !options.length || phase === 'pending'} onClick={() => void openDirectory()}>{t("打开目录")}{phase && <svg className={`detail-action-feedback ${phase === 'pending' ? 'is-pending' : ''}`} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true">{phase === 'pending' ? <path d="M16 10a6 6 0 1 1-6-6" /> : <path d="m4 10 4 4 8-8" />}</svg>}</button>
    </div>
    {error && <p className="issue-directory-error" role="alert" title={t(error)}>{t(error)}</p>}
  </div>
}

export const ProblemCenter = memo(function ProblemCenter({ sources, active = true }: { sources: string[]; active?: boolean }) {
  const locale = useLocale()
  const [status, setStatus] = useState<ProblemStatus>('open')
  const [source, setSource] = useState('all')
  const [query, setQuery] = useState('')
  const [kind, setKind] = useState<ProblemKindFilter>('all')
  const [page, setPage] = useState(1)
  const [problems, setProblems] = useState<LibraryProblem[]>([])
  const [selected, setSelected] = useState<string[]>([])
  const [loading, setLoading] = useState(realFilmsAvailable)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState<UiText>('')
  const [busy, setBusy] = useState(false)
  const [confirmation, setConfirmation] = useState<{ ids: string[]; directoryCount: number; action: 'mark' | 'restore' | 'clear' } | null>(null)
  const request = useRef(0)
  const mounted = useRef(false)

  useEffect(() => { if (!active) setConfirmation(null) }, [active])

  const refresh = useCallback(async () => {
    if (!realFilmsAvailable) return
    const revision = ++request.current
    try {
      const rows = await loadLibraryProblems()
      if (mounted.current && request.current === revision) { setProblems(rows); setError('') }
    } catch (reason) {
      if (mounted.current && request.current === revision) setError(problemError(reason))
    } finally {
      if (mounted.current && request.current === revision) setLoading(false)
    }
  }, [])
  useEffect(() => {
    mounted.current = true
    void refresh()
    window.addEventListener('real-library-changed', refresh)
    return () => { mounted.current = false; request.current++; window.removeEventListener('real-library-changed', refresh) }
  }, [refresh])

  const groups = useMemo(() => filterProblemGroups(groupDirectoryProblems(problems), status, source, query, kind, problem => [t(problemLabel(problem)), t(problemReasonMessage(problem.reason))]), [problems, status, source, query, kind, locale])
  const pageCount = Math.max(1, Math.ceil(groups.length / pageSize))
  const currentPage = Math.min(page, pageCount)
  const pageGroups = useMemo(() => groups.slice((currentPage - 1) * pageSize, currentPage * pageSize), [groups, currentPage])
  const filtered = useMemo(() => [...new Map(pageGroups.flatMap(group => group.items.map(item => [item.problem.id, item.problem] as const))).values()], [pageGroups])
  const selectedGroups = selectedProblemGroups(selected, pageGroups)
  const visibleSelection = problemGroupIssueIds(selectedGroups)
  const selectedDirectoryCount = selectedGroups.length
  useEffect(() => {
    setSelected(current => selectedProblemGroups(current, pageGroups).map(group => group.key))
    setConfirmation(null)
    if (page !== currentPage) setPage(currentPage)
  }, [pageGroups, page, currentPage])
  const resetPage = () => { setPage(1); setSelected([]); setConfirmation(null); setNotice('') }
  const kindOptions = useMemo(() => [...new Set(problems.map(problem => problem.kind))], [problems])
  const sourceOptions = useMemo(() => {
    const names = new Map<string, string>()
    for (const problem of problems) for (const item of problem.sources) names.set(item.id, item.name)
    if (!realFilmsAvailable) sources.forEach(name => names.set(name, name))
    return [...names].map(([id, name]) => ({ id, name }))
  }, [problems, sources])

  const actionGuard = useRef(false)
  const updateProblems = async (requested: string[], action: 'mark' | 'restore' | 'clear', directoryCount = 1) => {
    if (actionGuard.current) return
    const ids = visibleProblemSelection(requested, filtered, status)
    if (!ids.length) { setConfirmation(null); return }
    actionGuard.current = true
    setBusy(true); setError(''); setNotice('')
    try {
      await (action === 'clear' ? clearLibraryProblems(ids) : action === 'restore' ? restoreLibraryProblems(ids) : markLibraryProblems(ids))
      if (!mounted.current) return
      request.current++
      const changed = new Set(ids)
      setProblems(current => action === 'clear' ? current.filter(problem => !changed.has(problem.id)) : current.map(problem => changed.has(problem.id) ? { ...problem, handled: action === 'mark' } : problem))
      setSelected([])
      setConfirmation(null); setNotice(uiMessage(action === 'clear' ? '已清除所选 {0} 个对象的问题纪录，再次扫描会重新检查。' : action === 'restore' ? '已还原所选 {0} 个对象的问题。' : '已处理所选 {0} 个对象的问题。', [directoryCount]))
    } catch (reason) { if (mounted.current) setError(problemError(reason)) }
    finally { actionGuard.current = false; if (mounted.current) setBusy(false) }
  }
  const statusItems: { key: ProblemStatus; label: string; count: number }[] = [
    { key: 'open', label: t("待处理"), count: problems.filter(problem => !problem.handled).length },
    { key: 'resolved', label: t("已处理"), count: problems.filter(problem => problem.handled).length },
  ]
  const hasFilter = Boolean(query.trim() || source !== 'all' || kind !== 'all' || status !== 'open')

  return <section className="issues-page" aria-labelledby="issues-title">
    <div className="issues-heading">
      <div><h1 id="issues-title">{t("问题中心")}</h1><p>{t("集中处理片库问题")}</p></div>
      <div className="issues-total"><strong>{String(statusItems[0].count).padStart(2, '0')}</strong><span>{t("项待处理")}</span></div>
    </div>
    <div className="issues-toolbar">
      <div className="issues-tabs" role="group" aria-label={t("问题状态")}>
        {statusItems.map(item => <button key={item.key} type="button" disabled={busy} aria-pressed={status === item.key} className={status === item.key ? 'active' : ''} onClick={() => { setStatus(item.key); resetPage() }}>{item.label}<span>{item.count}</span></button>)}
      </div>
      <div className="issues-filters">
        <label className="issues-source">{t("片库")}<AppSelect disabled={busy} value={source} onChange={event => { setSource(event.target.value); resetPage() }} aria-label={t("问题片库")}><option value="all">{t("全部片库")}</option>{sourceOptions.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</AppSelect></label>
        {(kindOptions.length > 1 || kind !== 'all') && <label className="issues-source">{t("类型")}<AppSelect disabled={busy} value={kind} onChange={event => { setKind(event.target.value as ProblemKindFilter); resetPage() }} aria-label={t("问题类型")}><option value="all">{t("全部类型")}</option>{[...new Set([...kindOptions, ...(kind !== 'all' ? [kind] : [])])].map(value => <option key={value} value={value}>{t(problemLabel({ kind: value }))}</option>)}</AppSelect></label>}
        <label className="issues-search"><span aria-hidden="true">⌕</span><input type="search" disabled={busy} value={query} onChange={event => { setQuery(event.target.value); resetPage() }} placeholder={t("搜索问题或对象")} aria-label={t("搜索问题或对象")} /></label>
      </div>
    </div>
    <div className="issues-list-heading"><span>{t("问题清单")}<small>{t("{0}个对象", [groups.length])}</small></span><div className="issues-bulk-actions">
      <button type="button" disabled={!pageGroups.length || loading || busy} aria-pressed={pageGroups.length > 0 && selectedDirectoryCount === pageGroups.length} onClick={() => { setSelected(current => toggleAllProblemGroups(current, pageGroups)); setConfirmation(null) }}>{t("全选")}</button>
      <button type="button" disabled={!selectedDirectoryCount || busy} onClick={() => setConfirmation({ ids: visibleSelection, directoryCount: selectedDirectoryCount, action: status === 'resolved' ? 'restore' : 'mark' })}>{status === 'resolved' ? t("批量还原") : t("批量处理")}{t("{0}项", [selectedDirectoryCount])}</button>
      <button type="button" className="issues-clear" disabled={!selectedDirectoryCount || loading || busy} onClick={() => setConfirmation({ ids: visibleSelection, directoryCount: selectedDirectoryCount, action: 'clear' })}>{t("清除{0}项纪录", [selectedDirectoryCount])}</button>
    </div></div>
    {confirmation && <div className="issues-confirm" role="group" aria-label={confirmation.action === 'clear' ? t("确认清除纪录") : confirmation.action === 'restore' ? t("确认还原问题") : t("确认标记问题")}>
      <p>{t(confirmation.action === 'clear' ? '清除所选 {0} 个对象的问题纪录？再次扫描时会重新检查。' : confirmation.action === 'restore' ? '将所选 {0} 个对象的问题还原为待处理？' : '将所选 {0} 个对象的问题标记为已处理？', [confirmation.directoryCount])}</p>
      <div><button type="button" disabled={busy} onClick={() => setConfirmation(null)}>{t("取消")}</button><button type="button" disabled={busy} onClick={() => void updateProblems(confirmation.ids, confirmation.action, confirmation.directoryCount)}>{busy ? t("正在保存…") : t(confirmation.action === 'clear' ? '确认清除 {0} 项' : confirmation.action === 'restore' ? '确认还原 {0} 项' : '确认处理 {0} 项', [confirmation.directoryCount])}</button></div>
    </div>}
    {error && <div className="issues-feedback" role="alert"><span>{t(error)}</span><button type="button" disabled={busy || loading} onClick={() => void refresh()}>{t("重新读取")}</button></div>}
    {notice && <p className="issues-feedback" role="status">{t(notice)}</p>}
    {loading ? <div className="issues-empty" role="status"><h2>{t("正在读取问题资料…")}</h2></div> : filtered.length ? <><div className="issues-list">
      {pageGroups.map(group => {
        const groupIds = group.items.map(item => item.problem.id)
        const isSelected = selected.includes(group.key)
        const directoryItem = group.items.find(item => (item.problem.directory_options?.length ?? 0) > item.directoryIndex) ?? group.items[0]
        const reasons = [...new Set(group.items.map(item => t(problemReasonMessage(item.problem.reason))).filter(Boolean))]
        const sourceNames = [...new Set(group.items.flatMap(item => item.problem.sources.filter(sourceItem => !group.sourceId || sourceItem.id === group.sourceId).map(sourceItem => sourceItem.name)))]
        const reason = reasons.join(' ')
        const duplicateDirectories = [...new Set(group.items.filter(item => item.problem.kind === 'film_code_conflict').flatMap(item => item.problem.directories ?? []))].join('、')
        const toggleSelected = () => { setSelected(current => current.includes(group.key) ? current.filter(key => key !== group.key) : [...current, group.key]); setConfirmation(null) }
        return <article className={'issue-row' + (isSelected ? ' is-selected' : '')} key={group.key} data-app-press-surface={!busy ? true : undefined} onClick={event => {
          if (busy || !isProblemSelectionSurface(event.target)) return
          toggleSelected()
        }}>
        <div className="issue-row-heading">
          <label className="issue-select"><input type="checkbox" disabled={!groupIds.length || busy} checked={isSelected} onChange={toggleSelected} aria-label={t("选择 {0} 的问题", [group.title])} /></label>
          <div className="issue-object"><h2 title={group.title}>{group.title}</h2><div className="issue-type-tags">{group.items.map(({ problem }) => problem.handled ?
            <span key={problem.id} className="issue-type-tag handled">{t(problemLabel(problem))}<span>{t("已处理")}</span></span> :
            <button key={problem.id} type="button" className="issue-type-tag" disabled={busy} aria-label={t("处理 {0} 的{1}", [group.title, t(problemLabel(problem))])} onClick={() => void updateProblems([problem.id], 'mark')}>{t(problemLabel(problem))}<span>{t("待处理")}</span></button>)}</div></div>
          <div className="issue-row-actions">
            <ProblemDirectoryAction key={`${directoryItem.problem.id}:${directoryItem.directoryIndex}:${JSON.stringify(directoryItem.problem.directory_options)}`} problem={directoryItem.problem} directoryIndex={directoryItem.directoryIndex} title={group.title} />
            {status === 'open' && !!groupIds.length && <button type="button" disabled={busy} onClick={() => void updateProblems(groupIds, 'mark')}>{t("标记已处理")}</button>}</div>
        </div>
        <div className="issue-evidence"><p className="issue-library" title={sourceNames.join('、')}><span>{t("片库：")}</span>{sourceNames.join('、')}</p><p className="issue-reasons" title={reason}>{reason}</p>{duplicateDirectories && <p className="issue-duplicate-directories" title={duplicateDirectories}><span>{t("重复编号目录：")}</span>{duplicateDirectories}</p>}</div>
      </article>})}
    </div><ListPagination label={t("问题清单分页")} total={groups.length} pageSize={pageSize} page={currentPage} unit="个对象" disabled={busy} resetKey={JSON.stringify([status, source, query, kind])} onChange={next => { setPage(next); setSelected([]); setConfirmation(null); setNotice(''); document.getElementById('issues-title')?.scrollIntoView({ block: 'start' }) }} /></> : !error && <div className="issues-empty" role="status">
      <div className="issues-empty-symbol" aria-hidden="true"><span>✦</span></div>
      <h2>{hasFilter ? t("没有符合条件的问题") : t("暂无问题记录")}</h2>
      {hasFilter && <p>{t("可以调整搜索或筛选条件。")}</p>}
      {hasFilter && <button type="button" onClick={() => { setQuery(''); setSource('all'); setKind('all'); setStatus('open'); resetPage() }}>{t("清除筛选")}</button>}
    </div>}
  </section>
})
