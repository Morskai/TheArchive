import { t, useLocale } from '../i18n/react'
import { useEffect, useState, useRef } from 'react'
import { realFilmsAvailable } from '../data/realFilms'
import { defaultOptions, loadApplicationOptions, saveApplicationOptions, type ApplicationOptions as Options } from '../data/applicationOptions'

export function ApplicationOptions() {
  useLocale()
  const [options, setOptions] = useState(defaultOptions)
  const [ready, setReady] = useState(!realFilmsAvailable)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [read, setRead] = useState(0)
  const guard = useRef(false)
  useEffect(() => {
    if (!realFilmsAvailable) return
    let active = true
    loadApplicationOptions().then(value => { if (active) { setOptions(value); setReady(true); setError('') } })
      .catch(() => { if (active) setError('运行设置读取失败。') })
    return () => { active = false }
  }, [read])
  const save = async (patch: Partial<Options>) => {
    if (guard.current || !ready) return
    const next = { ...options, ...patch }
    if (!realFilmsAvailable) { setOptions(next); return }
    guard.current = true; setBusy(true); setError('')
    try { await saveApplicationOptions(next); setOptions(next) }
    catch { setError('设置未保存，请重试。') }
    finally { guard.current = false; setBusy(false) }
  }
  return <>
    <div className="settings-preference"><div><strong>{t("关闭窗口时退出程序")}</strong><span>{t("关闭此项则收起到托盘。")}</span></div><label className="settings-switch"><input type="checkbox" checked={options.closeWindowExits} disabled={!ready || busy} onChange={event => void save({ closeWindowExits: event.target.checked })} aria-label={t("关闭窗口时退出程序")} /><span aria-hidden="true" /></label></div>
    {error && <p className="settings-backup-feedback settings-error" role="alert">{t(error)}{!ready && <button type="button" className="settings-link" onClick={() => setRead(value => value + 1)}>{t("重试")}</button>}</p>}
  </>
}
