import { t, message as uiMessage, useLocale, getLanguage, type UiText } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import { createDataBackup, deleteDataBackup, listDataBackups, nativeBackupsAvailable, restoreDataBackup, type BackupInfo } from '../data/backup'

const restoreNoticeKey = 'film-library-restore-complete'

function explainBackupError(error: unknown): string {
  const detail = String(error)
  if (detail === 'backup_delete_failed') return '无法删除所选备份，请检查该备份是否仍存在及应用数据目录的访问权限。'
  if (/backup missing table|no such table/.test(detail)) return '备份缺少必要的资料表，已停止恢复。'
  if (/foreign key/.test(detail)) return '备份中的资料关联不完整，已停止恢复。'
  if (/integrity|not a database|unsupported backup version/.test(detail)) return '备份损坏或版本不兼容，已停止恢复。'
  if (/pending rollback file/.test(detail)) return '发现上次恢复留下的保护文件。为避免覆盖资料，已停止恢复。'
  if (/create snapshot|sync snapshot|publish snapshot/.test(detail)) return '无法在应用数据目录写入备份或恢复保护文件，操作已停止。'
  if (/unable to open database file/.test(detail)) return '无法读取应用数据或所选备份文件，请检查应用数据目录。'
  if (/database is locked/.test(detail)) return '软件资料库正在被占用，请稍后重试。'
  if (/Permission denied|Access is denied|拒绝访问/.test(detail)) return '应用数据目录的文件访问被拒绝，请检查权限或磁盘状态。'
  return '备份操作未完成，请检查软件资料目录后重试。'
}

export function BackupSection() {
  useLocale()
  const [backups, setBackups] = useState<BackupInfo[]>([])
  const [loading, setLoading] = useState(nativeBackupsAvailable)
  const [busy, setBusy] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [pendingRestore, setPendingRestore] = useState<BackupInfo | null>(null)
  const [pendingDelete, setPendingDelete] = useState<BackupInfo | null>(null)
  const [message, setMessage] = useState(() => {
    if (sessionStorage.getItem(restoreNoticeKey) === '1') return '软件资料已恢复，页面状态已重新载入。'
    return ''
  })
  const [error, setError] = useState<UiText>('')
  const dialogRef = useRef<HTMLDialogElement>(null)
  const cancelRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    sessionStorage.removeItem(restoreNoticeKey)
    if (!nativeBackupsAvailable) return
    let active = true
    listDataBackups()
      .then(rows => { if (active) setBackups(rows) })
      .catch(reason => { if (active) setError(uiMessage("无法读取应用内备份列表：{0}", [uiMessage(explainBackupError(reason))])) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [])

  useEffect(() => {
    const dialog = dialogRef.current
    if (!dialog) return
    if ((pendingRestore || pendingDelete) && !dialog.open) {
      dialog.showModal()
      cancelRef.current?.focus()
    } else if (!pendingRestore && !pendingDelete && dialog.open) {
      dialog.close()
    }
  }, [pendingRestore, pendingDelete])

  const createBackup = async () => {
    setBusy(true)
    setError('')
    setMessage('')
    try {
      await createDataBackup()
      setMessage('应用内备份已建立。')
    } catch (reason) {
      setError(uiMessage("建立备份失败：{0}", [uiMessage(explainBackupError(reason))]))
      setBusy(false)
      return
    }
    try {
      setBackups(await listDataBackups())
    } catch (reason) {
      setError(uiMessage("备份已建立，但列表刷新失败：{0}", [uiMessage(explainBackupError(reason))]))
    } finally {
      setBusy(false)
    }
  }

  const confirmRestore = async () => {
    if (!pendingRestore) return
    const backupId = pendingRestore.id
    setPendingRestore(null)
    setBusy(true)
    setError('')
    setMessage('')
    try {
      await restoreDataBackup(backupId)
    } catch (reason) {
      setError(uiMessage("恢复失败：{0}", [uiMessage(explainBackupError(reason))]))
      setBusy(false)
      return
    }
    try { sessionStorage.setItem(restoreNoticeKey, '1') } catch { /* Reload still clears old in-memory state. */ }
    window.location.reload()
  }

  const selected = backups.find(backup => backup.id === selectedId) ?? null
  const orderedBackups = [...backups].sort((a, b) => b.created_at_unix_ms - a.created_at_unix_ms || b.id.localeCompare(a.id))

  const confirmDelete = async () => {
    if (!pendingDelete || busy) return
    const backupId = pendingDelete.id
    setPendingDelete(null)
    setBusy(true)
    setError('')
    setMessage('')
    try {
      await deleteDataBackup(backupId)
      setBackups(current => current.filter(backup => backup.id !== backupId))
      setSelectedId(null)
      setMessage('所选备份已删除。')
    } catch (reason) {
      setError(uiMessage("删除失败：{0}", [uiMessage(explainBackupError(reason))]))
    } finally {
      setBusy(false)
    }
  }

  return <section className="settings-block" aria-labelledby="settings-backup-title">
    <div className="settings-block-head"><div><h2 id="settings-backup-title">{t("备份与恢复")}</h2><p>{t("管理软件使用数据")}</p></div></div>
    <div className="settings-backup-state">
      <strong>{!nativeBackupsAvailable ? t("浏览器预览无法使用原生备份") : loading ? t("正在读取应用内备份…") : backups.length ? t("应用内备份 {0} 份", [backups.length]) : t("暂无应用内备份")}</strong>
      <span>{t("备份软件资料与使用记录。")}</span>
    </div>
    {nativeBackupsAvailable && backups.length > 0 && <fieldset className="settings-backup-list" disabled={busy}>
      <legend>{t("选择备份 · 最新在前")}</legend>
      {orderedBackups.map((backup, index) => <label key={backup.id}>
        <input type="radio" name="restore-backup" value={backup.id} checked={selectedId === backup.id} onChange={() => setSelectedId(backup.id)} />
        <span>{new Date(backup.created_at_unix_ms).toLocaleString(getLanguage())} <small>{t("编号 {0}", [backup.id.slice(0, 8)])}</small></span>{index === 0 && <small className="settings-backup-latest">{t("最新")}</small>}
      </label>)}
    </fieldset>}
    <div className="settings-inline-actions">
      <button type="button" className="settings-action" disabled={!nativeBackupsAvailable || loading || busy} onClick={createBackup}>{t("立即备份")}</button>
      <button type="button" className="settings-action" disabled={!nativeBackupsAvailable || loading || busy || !selected} onClick={() => selected && setPendingRestore(selected)}>{t("恢复所选")}</button>
      <button type="button" className="settings-action settings-danger" disabled={!nativeBackupsAvailable || loading || busy || !selected} onClick={() => selected && setPendingDelete(selected)}>{t("删除所选")}</button>
    </div>
    {message && <p className="settings-backup-feedback" role="status">{t(message)}</p>}
    {error && <p className="settings-backup-feedback settings-error" role="alert">{t(error)}</p>}
    <dialog ref={dialogRef} className="settings-restore-dialog" aria-labelledby="restore-dialog-title" aria-describedby="restore-dialog-description" onClose={() => { setPendingRestore(null); setPendingDelete(null) }}>
      {pendingRestore && <><h3 id="restore-dialog-title">{t("确认恢复软件资料")}</h3>
        <p id="restore-dialog-description">{t("将以 {0}的备份替换当前软件资料。恢复前会先保护当前资料。", [new Date(pendingRestore.created_at_unix_ms).toLocaleString(getLanguage())])}</p>
        <div className="settings-restore-actions"><button ref={cancelRef} type="button" className="settings-action" onClick={() => setPendingRestore(null)}>{t("取消")}</button><button type="button" className="settings-primary" onClick={confirmRestore}>{t("确认恢复")}</button></div>
      </>}
      {pendingDelete && <><h3 id="restore-dialog-title">{t("删除所选备份")}</h3>
        <p id="restore-dialog-description">{t("将删除 {0}的应用内备份。当前软件资料、其他备份及磁盘媒体不受影响。", [new Date(pendingDelete.created_at_unix_ms).toLocaleString(getLanguage())])}</p>
        <div className="settings-restore-actions"><button ref={cancelRef} type="button" className="settings-action" onClick={() => setPendingDelete(null)}>{t("取消")}</button><button type="button" className="settings-action settings-danger" onClick={confirmDelete}>{t("确认删除")}</button></div>
      </>}
    </dialog>
  </section>
}
