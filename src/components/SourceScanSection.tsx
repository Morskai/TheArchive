import { sourceAddedMessage as sourceAddedNotice, sourcePreviewMessage as sourcePreviewNotice, sourceRefreshMessage as sourceRefreshNotice } from '../i18n/feedback'
import { t, message as uiMessage, useLocale, type UiText } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import {
  cancelLibraryPreview, changeLibrarySourceState, chooseLibraryRoot, confirmLibrarySource,
  listLibrarySources, nativeSourcesAvailable, openLibrarySourceDirectory, previewLibrarySource,
  type LibrarySource, type SourcePreview,
} from '../data/sources'
import { sourceSizeLabel } from '../data/deliveryPresentation'
import { localLaunchTime } from '../data/localDates'
import { displayLibraryPath } from '../data/libraryPath'
import { invalidateChangedCovers, refreshMissingCovers } from '../data/covers'
import { notifyRealLibraryChanged } from '../data/playback'
import { IgnoredDirectoryNames } from './IgnoredDirectoryNames'


type Step = 'idle' | 'choosing' | 'enumerating' | 'scanning' | 'confirming' | 'updating' | 'opening'
type Confirmation = { source: LibrarySource }

function explainError(error: unknown): string {
  switch (String(error)) {
    case 'wrong_root': return '目录不匹配，请重新选择。'
    case 'already_bound': return '目录已添加，可直接扫描。'
    case 'range_overlap': return '目录范围重叠，请检查已添加目录。'
    case 'source_paused': case 'source_inactive': return '目录已停用，请先启用。'
    case 'source_changed': case 'source_missing': return '目录状态已变化，请重新打开设置。'
    case 'root_unavailable': return '目录不存在或无读取权限。'
    case 'incomplete': return '目录检查不完整，暂无法保存。'
    case 'scan_changed': return '目录内容已变化，请重新扫描。'
    case 'preview_expired': return '检查结果已失效，请重新选择。'
    case 'protection_failed': return '资料保护失败，请检查应用数据目录。'
    case 'import_failed': return '保存失败，请重试。'
    case 'state_failed': return '目录状态未保存，请重试。'
    default: return '操作失败，请检查目录访问权限。'
  }
}

export function SourceScanSection({ guideActive = false }: { guideActive?: boolean }) {
  useLocale()
  const [sources, setSources] = useState<LibrarySource[]>([])
  const [step, setStep] = useState<Step>('idle')
  const [preview, setPreview] = useState<SourcePreview | null>(null)
  const [error, setError] = useState<UiText>('')
  const [notice, setNotice] = useState<UiText>('')
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null)
  const guard = useRef(false)


  useEffect(() => {
    if (!nativeSourcesAvailable) return
    let active = true
    listLibrarySources().then(rows => { if (active) setSources(rows) })
      .catch(() => { if (active) setError('目录列表暂时无法读取。') })
    return () => { active = false }
  }, [])

  const begin = (nextStep: Step) => {
    if (guard.current) return false
    guard.current = true
    setStep(nextStep); setError(''); setNotice('')
    return true
  }
  const finish = () => { guard.current = false; setStep('idle') }
  const refresh = async () => {
    try { setSources(await listLibrarySources()) }
    catch { setError('操作已完成，目录列表刷新失败。') }
  }

  const chooseAndPreview = async () => {
    if (!begin('choosing')) return
    setConfirmation(null); setPreview(null)
    try {
      const root = await chooseLibraryRoot()
      if (!root) { setNotice('已取消。'); return }
      setStep('enumerating')
      const result = await previewLibrarySource(root, null)
      setPreview(result)
      if (result.complete) setNotice(sourcePreviewNotice(result))
      else setError(uiMessage("检查未完成 · {0} 项问题", [result.issue_count]))
    } catch (reason) { setError(explainError(reason)) }
    finally { finish() }
  }

  const cancel = async () => {
    if (!begin('updating')) return
    try { if (preview?.token) await cancelLibraryPreview(preview.token) }
    catch { /* An expired preview cannot be confirmed. */ }
    finally {
      setPreview(null); setNotice('已取消。'); finish()
    }
  }

  const confirm = async () => {
    if (!preview?.complete || !preview.token || !begin('confirming')) return
    const resultPreview = preview
    setPreview(null)
    try {
      const confirmation = await confirmLibrarySource(resultPreview.token!)
      invalidateChangedCovers(confirmation.artwork_keys ?? [])
      setNotice(sourceAddedNotice(resultPreview, confirmation))
      refreshMissingCovers(confirmation.successful_artwork_keys ?? [])
      notifyRealLibraryChanged()
      await refresh()
    } catch (reason) { setError(explainError(reason)) }
    finally { finish() }
  }

  const changeState = async (source: LibrarySource, action: 'pause' | 'resume' | 'remove') => {
    if (!begin('updating')) return
    setConfirmation(null)
    try {
      await changeLibrarySourceState(source.id, action)
      notifyRealLibraryChanged()
      await refresh()
    } catch (reason) { setError(explainError(reason)) }
    finally { finish() }
  }

  const scanSource = async (source: LibrarySource) => {
    if (!begin('scanning')) return
    try {
      const result = await previewLibrarySource(source.root_path, source.id)
      if (!result.complete || !result.token) { setError(uiMessage("扫描未完成 · {0} 项问题", [result.issue_count])); return }
      const confirmation = await confirmLibrarySource(result.token)
      invalidateChangedCovers(confirmation.artwork_keys ?? [])
      setNotice(uiMessage("重新读取完成 · {0} 个视频{1}", [result.video_count, sourceRefreshNotice(confirmation)]))
      refreshMissingCovers(confirmation.successful_artwork_keys ?? [])
      notifyRealLibraryChanged()
      await refresh()
    } catch (reason) { setError(explainError(reason)) }
    finally { finish() }
  }

  const openSource = async (source: LibrarySource) => {
    if (!begin('opening')) return
    try {
      await openLibrarySourceDirectory(source.id)
    } catch (reason) {
      const code = String(reason)
      setError(code === 'directory_unavailable' ? '目录无法访问，请检查磁盘连接与读取权限。'
        : code === 'source_not_found' ? '此片库目录已移除，请刷新目录列表。'
          : '未能打开目录，请检查目录与系统状态后重试。')
    } finally { finish() }
  }

  const busy = step !== 'idle'
  const locked = busy || !!preview || !!confirmation
  const progress = step === 'choosing' ? t("请选择片库目录…") : step === 'enumerating' ? t("正在检查目录…")
    : step === 'scanning' ? t("正在重新读取目录…") : step === 'confirming' ? t("正在添加目录…") : step === 'updating' ? t("正在更新目录…") : step === 'opening' ? t("正在打开目录…") : ''
  const summary = progress || error || notice

  return <>
    <div className="settings-block-head"><div><h2 id="settings-libraries-title" tabIndex={-1}>{t("片库目录")}</h2><p>{t("添加片库目录，扫描并收录。")}</p></div>
      <div className="settings-source-controls"><IgnoredDirectoryNames disabled={locked} /><div className="settings-add-directory-wrap"><button type="button" className={`settings-action${guideActive ? ' is-guided' : ''}`} disabled={!nativeSourcesAvailable || locked} onClick={() => chooseAndPreview()}>{t("添加目录")}</button>{guideActive && <div className="scan-entry-guide" role="status"><svg viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M9 15V3m-4 4 4-4 4 4" /></svg><span>{t("点击添加目录")}</span></div>}</div></div>
    </div>
    <div className={`settings-library-list ${sources.length > 4 ? 'is-scrollable' : ''}`} tabIndex={sources.length > 4 ? 0 : undefined} role="region" aria-labelledby="settings-libraries-title">
      {sources.length ? sources.map(source => <div className="settings-library-row" key={source.id}>
        <button type="button" className="settings-library-mark" disabled={locked} title={t("打开目录：{0}", [source.name])} aria-label={t("打开片库目录：{0}", [source.name])} onClick={() => openSource(source)}><svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><path d="M3 7.5h7l2 2h9v9.5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V7.5Zm0 0V5a1 1 0 0 1 1-1h5" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" /></svg></button>
        <div><strong>{source.name}</strong><small className="settings-source-path" title={displayLibraryPath(source.root_path)}>{displayLibraryPath(source.root_path)}</small><small>{source.enabled ? t("已启用 · 共 {0} 部影片 · 占用 {1} · 扫描时间 {2}", [source.film_count, t(sourceSizeLabel(source.video_bytes)), source.last_scanned_at ? localLaunchTime(source.last_scanned_at) : t("未记录")]) : t("已停用")}</small></div>
        <span className="settings-library-actions">

          {source.enabled ? <><button type="button" className="settings-action" disabled={locked} onClick={() => scanSource(source)}>{t("扫描")}</button><button type="button" className="settings-action" disabled={locked} onClick={() => changeState(source, 'pause')}>{t("停用")}</button></>
            : <button type="button" className="settings-action" disabled={locked} onClick={() => changeState(source, 'resume')}>{t("启用")}</button>}
          <button type="button" className="settings-action" disabled={locked} onClick={() => { setNotice(''); setError(''); setConfirmation({ source }) }}>{t("移除")}</button>
        </span>
      </div>) : <div className="settings-library-empty">{t("暂无片库目录")}</div>}
    </div>
    {confirmation && <div className="settings-source-preview" role="group" aria-label={t("确认移除目录")}>
      <strong>{t("移除目录：{0}", [confirmation.source.name])}</strong>
      <p>{t("删除目录记录和扫描资料，保留人工资料及磁盘文件。再次添加时重新扫描。")}</p>
      <div className="settings-inline-actions"><button type="button" className="settings-action" onClick={() => { setConfirmation(null); setNotice('') }}>{t("取消")}</button><button type="button" className="settings-primary" onClick={() => changeState(confirmation.source, 'remove')}>{t("确认移除")}</button></div>
    </div>}
    {summary && !confirmation && <div className="settings-source-feedback">
      <div className="settings-source-feedback-row">
        <p className={`settings-backup-feedback ${error ? 'settings-error' : ''}`} role={error ? 'alert' : 'status'}>{t(summary)}</p>
        {preview && !busy && <div className="settings-inline-actions"><button type="button" className="settings-action" onClick={cancel}>{t("取消")}</button><button type="button" className="settings-primary" disabled={!preview.complete || !preview.token} onClick={confirm}>{t("确认添加")}</button></div>}
      </div>
    </div>}
  </>
}
