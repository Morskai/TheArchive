import { t, useLocale } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import { isTauri } from '@tauri-apps/api/core'
import { exportDiagnosticLogs, loadDiagnosticStatus, openDiagnosticDirectory, type DiagnosticStatus } from '../data/diagnostics'

export function DiagnosticSection() {
  useLocale()
  const [status, setStatus] = useState<DiagnosticStatus | null>(null)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [failed, setFailed] = useState(false)
  const guard = useRef(false)
  const desktop = isTauri()
  useEffect(() => {
    if (!desktop) return
    let active = true
    loadDiagnosticStatus().then(value => { if (active) setStatus(value) }).catch(() => { if (active) { setFailed(true); setNotice('日志状态暂时无法读取。') } })
    return () => { active = false }
  }, [desktop])
  const act = async (action: 'open' | 'export') => {
    if (guard.current) return
    guard.current = true; setBusy(true); setFailed(false); setNotice('')
    try {
      if (action === 'open') { await openDiagnosticDirectory(); setNotice('已请求打开日志目录。') }
      else { const path = await exportDiagnosticLogs(); if (path) setNotice('诊断日志已导出，可将文件交给开发者。') }
      try { setStatus(await loadDiagnosticStatus()) } catch { /* Keep the actual open/export outcome. */ }
    } catch { setFailed(true); setNotice(action === 'open' ? '日志目录未能打开，请稍后重试。' : '诊断日志未能导出，请检查保存目录后重试。') }
    finally { guard.current = false; setBusy(false) }
  }
  return <section className="settings-block" aria-labelledby="settings-diagnostics-title">
    <div className="settings-block-head"><div><h2 id="settings-diagnostics-title">{t("诊断日志")}</h2><p>{desktop ? t("默认开启，帮助定位程序错误") : t("请在桌面版查看和导出日志")}</p></div></div>
    <p className="settings-diagnostic-copy">{t("日志仅保存在本机，最多约 25 MB。导出包含诊断记录，不含 API 密钥、影片名称、演员资料或完整媒体路径。")}</p>
    {desktop && status && !status.available && <p className="settings-backup-feedback settings-error" role="status">{t("日志暂时无法写入，程序仍可继续使用。")}</p>}
    <div className="settings-save-row">
      <button type="button" className="settings-action" disabled={!desktop || busy} onClick={() => void act('open')}>{t("打开日志目录")}</button>
      <button type="button" className="settings-primary" disabled={!desktop || busy} onClick={() => void act('export')}>{busy ? t("正在处理…") : t("导出诊断日志")}</button>
    </div>
    {notice && <p className={`settings-backup-feedback${failed ? ' settings-error' : ''}`} role={failed ? 'alert' : 'status'}>{t(notice)}</p>}
  </section>
}
