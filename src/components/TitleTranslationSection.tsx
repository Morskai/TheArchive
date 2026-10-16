import { t, message as uiMessage, useLocale, type UiText } from '../i18n/react'
import { AppSelect } from './AppSelect'
import { useEffect, useRef, useState } from 'react'
import { realFilmsAvailable } from '../data/realFilms'
import { notifyRealLibraryChanged } from '../data/playback'
import { cancelTitleTranslation, clearTranslationMemory, deleteTranslationKey, loadTranslationKey, loadTranslationSettings, previewTitleTranslations, saveTranslationKey, saveTranslationSettings, startTitleTranslation, testTranslationConnection, translationJobStatus } from '../data/titleTranslationApi'
import { defaultTranslationIds, filterTranslationEntries, toggleVisibleTranslationIds, selectedTranslationCounts, selectedTranslationEntries, translationError, type TranslationJobStatus, type TranslationPreview, type TranslationSettings, type TranslationSettingsState, type TranslationStatusFilter } from '../data/titleTranslation'

const statusLabels = { manual: '已编辑', chinese: '中文原文', automatic: '已有翻译标题', local_memory: '可本地复用', stale: '原文已变化', pending: '需要 API', language_uncertain: '语言待确认' }

export function TitleTranslationSection() {
  useLocale()
  const [saved, setSaved] = useState<TranslationSettingsState | null>(null)
  const [draft, setDraft] = useState<TranslationSettings | null>(null)
  const [keyDraft, setKeyDraft] = useState('')
  const [showKey, setShowKey] = useState(false)
  const [keyLoading, setKeyLoading] = useState(false)
  const [keyNotice, setKeyNotice] = useState('')
  const [keyError, setKeyError] = useState('')
  const [preview, setPreview] = useState<TranslationPreview | null>(null)
  const [selected, setSelected] = useState<string[]>([])
  const [query, setQuery] = useState('')
  const [source, setSource] = useState('all')
  const [statusFilter, setStatusFilter] = useState<TranslationStatusFilter>('all')
  const [detectionNotice, setDetectionNotice] = useState('')
  const [expanded, setExpanded] = useState(true)
  const [force, setForce] = useState(false)
  const [job, setJob] = useState<TranslationJobStatus | null>(null)
  const [busy, setBusy] = useState(realFilmsAvailable)
  const [notice, setNotice] = useState<UiText>('')
  const [error, setError] = useState<UiText>('')
  const [confirmation, setConfirmation] = useState<number | null>(null)
  const guard = useRef(false)
  const mounted = useRef(false)
  const request = useRef(0)
  const clearButton = useRef<HTMLButtonElement>(null)
  const cancelButton = useRef<HTMLButtonElement>(null)
  const successes = useRef(0)
  const lastPublished = useRef(0)
  const running = Boolean(job?.running)
  const locked = busy || running || keyLoading
  const dirty = Boolean(draft && saved && JSON.stringify(draft) !== JSON.stringify(saved.settings))

  const refreshSettings = async (revision: number) => {
    const state = await loadTranslationSettings()
    if (mounted.current && revision === request.current) { setSaved(state); setDraft(state.settings) }
  }
  const acceptJob = (status: TranslationJobStatus) => {
    setJob(status)
    if (!status.running || (status.succeeded + status.reused > successes.current && Date.now() - lastPublished.current >= 4000)) {
      successes.current = status.succeeded + status.reused
      lastPublished.current = Date.now()
      notifyRealLibraryChanged()
    }
  }
  useEffect(() => {
    mounted.current = true
    if (!realFilmsAvailable) return () => { mounted.current = false }
    const revision = ++request.current
    Promise.all([loadTranslationSettings(), translationJobStatus()]).then(([state, status]) => {
      if (!mounted.current || revision !== request.current) return
      setSaved(state); setDraft(state.settings); setJob(status); successes.current = status.succeeded + status.reused
    }).catch(reason => { if (mounted.current) setError(translationError(reason)) }).finally(() => { if (mounted.current) setBusy(false) })
    return () => { mounted.current = false; request.current++ }
  }, [])
  useEffect(() => {
    if (!running) return
    let active = true
    let timer = 0
    let failures = 0
    const poll = async () => {
      const revision = request.current
      try {
        const status = await translationJobStatus()
        if (!active) return
        if (revision !== request.current) { timer = window.setTimeout(poll, 1200); return }
        failures = 0; acceptJob(status)
        if (status.running) timer = window.setTimeout(poll, 1200)
        else { setPreview(null); setSelected([]); void refreshSettings(request.current).catch(() => { if (active) setError('任务已结束，但状态统计暂时无法读取，请重新检测。') }) }
      } catch {
        if (!active) return
        failures++
        setError('翻译进度暂时无法读取；已保存的结果保留。')
        if (failures < 3) timer = window.setTimeout(poll, 1800)
      }
    }
    timer = window.setTimeout(poll, 1200)
    return () => { active = false; window.clearTimeout(timer) }
  }, [running])
  useEffect(() => { if (confirmation !== null) cancelButton.current?.focus() }, [confirmation])
  useEffect(() => {
    if (!realFilmsAvailable || !draft || !saved) return
    let active = true
    setKeyDraft(''); setShowKey(false); setKeyNotice(''); setKeyError('')
    if (draft.provider !== saved.settings.provider) { setKeyLoading(false); return }
    setKeyLoading(true)
    loadTranslationKey(saved.settings.provider).then(key => {
      if (active) setKeyDraft(key ?? '')
    }).catch(reason => { if (active) setKeyError(translationError(reason)) })
      .finally(() => { if (active) setKeyLoading(false) })
    return () => { active = false }
  }, [draft?.provider, saved?.settings.provider])

  const operate = async (action: (revision: number) => Promise<void>, allowRunning = false, keyAction = false) => {
    if (!realFilmsAvailable || guard.current || (running && !allowRunning)) return
    guard.current = true
    const revision = ++request.current
    setBusy(true); setError(''); setNotice('')
    if (keyAction) { setKeyError(''); setKeyNotice('') }
    try { await action(revision) }
    catch (reason) { if (mounted.current && revision === request.current) (keyAction ? setKeyError : setError)(translationError(reason)) }
    finally { guard.current = false; if (mounted.current && revision === request.current) setBusy(false) }
  }
  const detect = () => operate(async revision => {
    setConfirmation(null)
    const [result, state] = await Promise.all([previewTitleTranslations(), loadTranslationSettings()])
    if (!mounted.current || revision !== request.current) return
    setPreview(result); setSaved(state); setDraft(state.settings)
    setQuery(''); setSource('all'); setStatusFilter('all'); setExpanded(true)
    setSelected(defaultTranslationIds(result.entries, force))
    setDetectionNotice('检测完成；请选择本次处理的影片。')
  })
  const changeForce = (value: boolean) => {
    setForce(value); setConfirmation(null); setNotice('')
    setSelected(preview ? defaultTranslationIds(preview.entries, value) : [])
  }
  const counts = selectedTranslationCounts(preview?.entries ?? [], selected, force)
  const eligible = selectedTranslationEntries(preview?.entries ?? [], (preview?.entries ?? []).map(entry => entry.film_id), force)
  const eligibleIds = new Set(eligible.map(entry => entry.film_id))
  const selectedIds = new Set(selected)
  const visible = filterTranslationEntries(preview?.entries ?? [], query, source, statusFilter)
  const visibleIds = visible.filter(entry => eligibleIds.has(entry.film_id)).map(entry => entry.film_id)
  const allVisibleSelected = visibleIds.length > 0 && visibleIds.every(id => selectedIds.has(id))
  const sources = [...new Map((preview?.entries ?? []).flatMap(entry => (entry.sources ?? []).map(item => [item.id, item] as const))).values()]
  const stats = saved?.stats

  return <section className="settings-block title-translation" aria-labelledby="title-translation-heading" aria-busy={busy}>
    <div className="settings-block-head"><div><h2 id="title-translation-heading">{t("影片标题翻译")}</h2><p>{t("翻译影片标题，管理翻译记忆。")}</p></div></div>
    {!realFilmsAvailable ? <p className="settings-backup-feedback">{t("请在桌面版配置翻译服务并检测已保存影片；浏览器预览不连接翻译服务。")}</p> : !draft ? <div className="settings-save-row"><span role="status">{busy ? t("正在读取翻译设置…") : t("翻译设置暂时无法读取。")}</span><button className="settings-action" disabled={busy} onClick={() => void operate(revision => refreshSettings(revision))}>{t("重新读取")}</button></div> : <>
      <div className="translation-configuration">
        <label className="translation-enabled"><input type="checkbox" checked={draft.enabled} disabled={locked} onChange={event => { setDraft({ ...draft, enabled: event.target.checked }); setConfirmation(null) }} />{t("启用标题翻译")}</label>
        <label>{t("翻译服务")}<AppSelect aria-label={t("翻译服务商")} value={draft.provider} disabled={locked} onChange={event => { setKeyDraft(''); setDraft({ ...draft, provider: event.target.value as TranslationSettings['provider'] }); setConfirmation(null); setError(''); setNotice('') }}><option value="deepl">DeepL</option><option value="google">Google Cloud Translation</option><option value="azure">Microsoft Azure Translator</option><option value="baidu">{t("百度翻译开放平台")}</option></AppSelect></label>
        {draft.provider === 'deepl' && <label>{t("DeepL API 类型")}<AppSelect aria-label={t("DeepL API 类型")} disabled={locked} value={draft.deepl_plan} onChange={event => setDraft({ ...draft, deepl_plan: event.target.value as TranslationSettings['deepl_plan'] })}><option value="free">Free</option><option value="pro">Pro</option></AppSelect></label>}
        {draft.provider === 'azure' && <label>{t("Azure 区域")}<input aria-label={t("Azure 区域")} disabled={locked} value={draft.azure_region} maxLength={64} placeholder={t("global 或 eastasia")} onChange={event => setDraft({ ...draft, azure_region: event.target.value.trim().toLowerCase() })} /></label>}
        {draft.provider === 'baidu' && <label>{t("百度 APP ID")}<input aria-label={t("百度 APP ID")} disabled={locked} value={draft.baidu_app_id} maxLength={64} inputMode="numeric" placeholder={t("开放平台 APP ID")} onChange={event => setDraft({ ...draft, baidu_app_id: event.target.value.trim() })} /></label>}
        <div className="translation-target"><span>{t("目标语言")}</span><strong>{t("简体中文")}</strong></div>
        <button className="settings-primary" disabled={locked || !dirty} onClick={() => void operate(async revision => { const state = await saveTranslationSettings(draft); if (mounted.current && revision === request.current) { setSaved(state); setDraft(state.settings); setPreview(null); setSelected([]); setConfirmation(null); setNotice('翻译设置已保存；已有中文标题继续保留。') } })}>{t("保存翻译设置")}</button>
      </div>
      <div className="translation-key settings-fields">
        <label>{draft.provider === 'baidu' ? t("百度密钥") : 'API Key'} <span>{dirty ? t("请先保存翻译设置") : keyLoading ? t("正在读取…") : saved?.key_configured ? t("已配置") : t("未配置")}</span>
          <span className="translation-key-input"><input type={showKey ? 'text' : 'password'} autoComplete="off" spellCheck={false} disabled={locked || dirty} value={keyDraft} onChange={event => { setKeyDraft(event.target.value); setKeyNotice(''); setKeyError('') }} placeholder={saved?.key_configured ? t("输入新密钥以更换") : t("输入当前服务的 API Key")} aria-label={t("翻译 API Key")} /><button type="button" className="translation-key-toggle" disabled={keyLoading || dirty} aria-label={showKey ? t("隐藏密钥") : t("显示密钥")} aria-pressed={showKey} onClick={() => setShowKey(value => !value)}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{showKey ? <><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z" /><circle cx="12" cy="12" r="3" /></> : <><path d="M3 5 21 19M2 12s3.5-7 10-7c2.7 0 5 1.2 6.8 2.8M22 12s-3.5 7-10 7c-2.7 0-5-1.2-6.8-2.8M10 9.8a3 3 0 0 1 4.2 4.2" /></>}</svg></button></span>
        </label>
        <div className="translation-key-actions">
          <button className="settings-action" disabled={locked || dirty || !keyDraft.trim()} onClick={() => void operate(async revision => { const key = keyDraft.trim(); await saveTranslationKey(draft.provider, key); await refreshSettings(revision); if (mounted.current && revision === request.current) { setKeyDraft(key); setKeyNotice('密钥已保存到 Windows 安全凭据存储。') } }, false, true)}>{saved?.key_configured ? t("更换密钥") : t("保存密钥")}</button>
          <button className="settings-action" disabled={locked || dirty || !saved?.key_configured} onClick={() => void operate(async revision => { await deleteTranslationKey(draft.provider); setKeyDraft(''); setShowKey(false); await refreshSettings(revision); if (mounted.current && revision === request.current) setKeyNotice('当前服务的密钥已删除。') }, false, true)}>{t("删除密钥")}</button>
          <button className="settings-action" disabled={locked || dirty || !saved?.key_configured} onClick={() => void operate(async revision => { await testTranslationConnection(draft.provider); if (mounted.current && revision === request.current) setKeyNotice('测试连接成功。') }, false, true)}>{t("测试连接")}</button>
        </div>
      </div>
      {keyNotice && <p className="settings-backup-feedback translation-key-feedback" role="status">{t(keyNotice)}</p>}
      {keyError && <p className="settings-backup-feedback settings-error translation-key-feedback" role="alert">{t(keyError)}</p>}
      <p className="translation-help">{t("测试连接会向所选服务发送一段固定测试文字，可能消耗少量额度。")}</p>
      <div className="translation-summary-row"><dl className="translation-statistics"><div><dt>{t("待翻译")}</dt><dd>{stats?.pending ?? '—'}</dd></div><div><dt>{t("翻译记忆")}</dt><dd>{stats?.memory ?? '—'}</dd></div></dl><button ref={clearButton} className="settings-action settings-danger" disabled={locked || !stats || stats.memory === 0} aria-expanded={confirmation !== null} onClick={() => { setConfirmation(stats?.memory ?? 0); setNotice(''); setError('') }}>{t("清除翻译记忆")}</button></div>
      {confirmation !== null && <div className="translation-clear-confirmation"><p>{t("清除 {0}条翻译记忆及对应译名？", [confirmation])}</p><div className="settings-cleanup-actions" role="group" aria-label={t("确认清除翻译记忆")} onKeyDown={event => { if (event.key === 'Escape') { setConfirmation(null); clearButton.current?.focus() } }}><button className="settings-action settings-danger settings-cleanup-confirm" disabled={locked} onClick={() => void operate(async revision => { const count = confirmation; setConfirmation(null); const removed = await clearTranslationMemory(count); await refreshSettings(revision); if (mounted.current) { setPreview(null); setSelected([]); notifyRealLibraryChanged(); setNotice(uiMessage("已清除 {0} 条翻译记忆。", [removed])); clearButton.current?.focus() } })}>{t("确认清除")}</button><button ref={cancelButton} className="settings-action settings-cleanup-cancel" disabled={busy} onClick={() => { setConfirmation(null); clearButton.current?.focus() }}>{t("取消")}</button></div></div>}
      <div className="translation-preview-controls"><button className="settings-action" disabled={locked || dirty} onClick={() => void detect()}>{t("检测需要翻译的影片")}</button><label><input type="checkbox" checked={force} disabled={locked} onChange={event => changeForce(event.target.checked)} />{t("使用当前服务重新翻译已有自动标题")}</label></div>
      {preview && <div className="translation-preview">
        {detectionNotice && <p role="status">{t(detectionNotice)}</p>}
        <div className="translation-selection"><button className="settings-link" disabled={locked || visibleIds.length === 0} onClick={() => setSelected(current => toggleVisibleTranslationIds(current, visibleIds))}>{allVisibleSelected ? t("取消全选") : t("全选")}</button><span>{t("已选 {0}部 / 可选 {1}部", [counts.candidates, eligible.length])}</span>
          <label className="translation-search"><input type="search" aria-label={t("搜索翻译影片")} value={query} onChange={event => setQuery(event.target.value)} placeholder={t("搜索影片")} /></label>
          <label className="translation-source"><AppSelect aria-label={t("筛选片库")} value={source} onChange={event => setSource(event.target.value)}><option value="all">{t("全部片库")}</option>{sources.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</AppSelect></label>
          <div className="translation-status-filters" role="group" aria-label={t("翻译状态筛选")}>
            <label><input type="checkbox" checked={statusFilter === 'translated'} onChange={event => setStatusFilter(event.target.checked ? 'translated' : 'all')} />{t("已翻译")}</label>
            <label><input type="checkbox" checked={statusFilter === 'uncertain'} onChange={event => setStatusFilter(event.target.checked ? 'uncertain' : 'all')} />{t("待确认")}</label>
          </div>
          <button className="settings-link translation-collapse" aria-expanded={expanded} aria-controls="translation-candidate-list" onClick={() => setExpanded(value => !value)}>{expanded ? t("收起列表") : t("展开列表")} <span aria-hidden="true">{expanded ? '⌃' : '⌄'}</span></button>
        </div>
        <div id="translation-candidate-list" hidden={!expanded}><ul className="translation-candidates">{visible.map(entry => { const selectable = eligibleIds.has(entry.film_id); return <li key={entry.film_id}><label><input type="checkbox" disabled={locked || !selectable} checked={selectable && selectedIds.has(entry.film_id)} onChange={event => setSelected(current => event.target.checked ? [...new Set([...current, entry.film_id])] : current.filter(id => id !== entry.film_id))} /><span>{entry.original_title}</span><small>{t(statusLabels[entry.status])}</small></label></li> })}</ul>{visible.length === 0 && <p className="translation-help" role="status">{preview.entries.length ? t("没有符合条件的影片。") : t("当前没有已保存的影片标题。")}</p>}</div>
        <button className="settings-primary" disabled={locked || dirty || !draft.enabled || counts.candidates === 0 || (counts.apiRequests > 0 && !saved?.key_configured)} onClick={() => void operate(async () => { setConfirmation(null); const result = await startTitleTranslation(selectedTranslationEntries(preview.entries, selected, force).map(entry => entry.film_id), force); if (mounted.current) { successes.current = 0; acceptJob(result); if (!result.running) { setPreview(null); await refreshSettings(request.current) } } })}>{force ? t("重新翻译所选自动标题") : t("开始翻译所选影片")}</button>{counts.apiRequests > 0 && !saved?.key_configured && <p className="translation-help">{t("本次需要 API，请先配置密钥。本地复用不需要密钥。")}</p>}
      </div>}
      {job && job.total > 0 && <div className="translation-job" role="status"><p>{job.running ? t("翻译进行中") : job.cancelled ? t("任务已取消；已完成结果保留") : t("任务已结束")} · {job.completed} / {job.total}</p><p>{t("成功 {0}· 失败 {1}· 本地复用 {2}· API 请求 {3}", [job.succeeded, job.failed, job.reused, job.api_requests])}</p><progress max={Math.max(job.total, 1)} value={job.completed} aria-label={t("标题翻译进度")} />{job.last_error && <p className="settings-error">{t(translationError(job.last_error))}</p>}{job.running && <button className="settings-action" disabled={busy} onClick={() => void operate(async () => { const result = await cancelTitleTranslation(); if (mounted.current) { acceptJob(result); setNotice('取消已请求；正在进行的请求结束后停止，已完成结果保留。') } }, true)}>{t("取消翻译")}</button>}<button className="settings-action translation-refresh" disabled={busy} onClick={() => void operate(async () => { const status = await translationJobStatus(); if (mounted.current) acceptJob(status) }, true)}>{t("刷新任务状态")}</button></div>}
    </>}
    {notice && <p className="settings-backup-feedback" role="status">{t(notice)}</p>}{error && <p className="settings-backup-feedback settings-error" role="alert">{t(error)}</p>}
  </section>
}
