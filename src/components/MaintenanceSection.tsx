import { t, useLocale } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import { realFilmsAvailable } from '../data/realFilms'
import { clearGeneratedCache, clearUserData, dismissResetNotice, maintenanceError, readResetNotice, rememberResetNotice } from '../data/maintenance'
import { invalidateCovers } from '../data/covers'

interface MaintenanceSectionProps { onDataCleared: () => void }
type Cleanup = 'cache' | 'data'

export function MaintenanceSection({ onDataCleared }: MaintenanceSectionProps) {
  useLocale()
  const [pending, setPending] = useState<Cleanup | null>(null)
  const [busy, setBusy] = useState<Cleanup | null>(null)
  const [message, setMessage] = useState(() => readResetNotice() === 'complete' ? '已恢复初始状态，标题翻译记忆已保留。' : '')
  const [error, setError] = useState(() => readResetNotice() === 'cleanup_pending' ? maintenanceError('reset_cleanup_pending') : '')
  const guard = useRef(false)
  const cancelRef = useRef<HTMLButtonElement>(null)
  const cacheRef = useRef<HTMLButtonElement>(null)
  const dataRef = useRef<HTMLButtonElement>(null)
  const dialogRef = useRef<HTMLDialogElement>(null)
  const restoreFocus = useRef<Cleanup | null>(null)

  useEffect(() => { dismissResetNotice() }, [])

  useEffect(() => {
    const dialog = dialogRef.current
    if (pending === 'data' && dialog && !dialog.open) dialog.showModal()
    else if (pending !== 'data' && dialog?.open) dialog.close()
    if (pending) cancelRef.current?.focus()
    else if (!busy && restoreFocus.current) {
      (restoreFocus.current === 'cache' ? cacheRef : dataRef).current?.focus()
      restoreFocus.current = null
    }
  }, [pending, busy])

  const cancel = () => { restoreFocus.current = pending; setPending(null) }

  const confirm = async () => {
    if (!pending || guard.current || !realFilmsAvailable) return
    const operation = pending
    restoreFocus.current = operation
    guard.current = true
    setPending(null); setBusy(operation); setError(''); setMessage('')
    try {
      if (operation === 'cache') {
        await clearGeneratedCache()
        invalidateCovers()
        setMessage('图片缓存已清空，图片会在需要时重新生成，无需重新扫描。')
      } else {
        await clearUserData()
        rememberResetNotice('complete')
        onDataCleared()
        setMessage('已恢复初始状态，标题翻译记忆已保留。')
      }
    } catch (reason) {
      if (operation === 'data' && String(reason) === 'reset_cleanup_pending') {
        rememberResetNotice('cleanup_pending')
        onDataCleared()
      }
      setError(maintenanceError(reason))
    }
    finally { guard.current = false; setBusy(null) }
  }

  const cleanupActions = (operation: Cleanup) => pending === operation && operation === 'cache' ?
    <div className="settings-cleanup-actions" role="group" aria-label={t("确认清空图片缓存")} onKeyDown={event => { if (event.key === 'Escape') cancel() }}>
      <button type="button" className="settings-primary" onClick={() => void confirm()}>{t("确认清空")}</button>
      <button ref={cancelRef} type="button" className="settings-action settings-cleanup-cancel" onClick={cancel}>{t("取消")}</button>
    </div> :
    <button ref={operation === 'cache' ? cacheRef : dataRef} type="button" className={`settings-action${operation === 'data' ? ' settings-danger' : ''}`} disabled={!realFilmsAvailable || Boolean(busy)} onClick={() => { setError(''); setMessage(''); setPending(operation) }}>{busy === operation ? operation === 'cache' ? t("正在清空…") : t("正在恢复…") : operation === 'cache' ? t("清空图片缓存") : t("恢复初始状态")}</button>

  return <section className="settings-block settings-maintenance" aria-labelledby="settings-maintenance-title" aria-busy={Boolean(busy)}>
    <div className="settings-block-head"><div><h2 id="settings-maintenance-title">{t("清除数据")}</h2><p>{t("管理图片缓存和使用数据")}</p></div></div>
    <div className="settings-maintenance-item"><div><strong>{t("图片缓存")}</strong><p>{t("清理软件的缓存图片，清除后缓存自动重建。")}</p></div>{cleanupActions('cache')}</div>
    <div className="settings-maintenance-item"><div><strong>{t("恢复初始状态")}</strong><p>{t("清除片库、演员的相关使用数据，恢复默认设置；保留标题翻译记忆。")}</p></div>{cleanupActions('data')}</div>
    {!realFilmsAvailable && <p className="settings-backup-feedback">{t("请在桌面版执行清理；浏览器预览不会清除真实缓存或资料。")}</p>}
    {message && <p className="settings-backup-feedback" role="status">{t(message)}</p>}
    {error && <p className="settings-backup-feedback settings-error" role="alert">{t(error)}</p>}
    <dialog ref={dialogRef} className="settings-restore-dialog" aria-labelledby="reset-dialog-title" aria-describedby="reset-dialog-description" onCancel={event => { event.preventDefault(); cancel() }}>
      <h3 id="reset-dialog-title">{t("警告")}</h3>
      <div id="reset-dialog-description">
        <p>{t("将清除全部影片、演员相关的软件数据，清除缓存、API凭据并恢复默认设置；保留翻译记忆与备份记录。")}</p>
      </div>
      <div className="settings-restore-actions"><button ref={pending === 'data' ? cancelRef : undefined} type="button" className="settings-action" onClick={cancel}>{t("取消")}</button><button type="button" className="settings-action settings-danger" onClick={() => void confirm()}>{t("确认恢复初始状态")}</button></div>
    </dialog>
  </section>
}
