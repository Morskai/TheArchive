import { t, useLocale } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '../data/diagnostics.ts'
import { realFilmsAvailable } from '../data/realFilms'

export function WindowCloseDialog() {
  useLocale()
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState('')
  const dialog = useRef<HTMLDialogElement>(null)
  const cancel = useRef<HTMLButtonElement>(null)
  const previous = useRef<HTMLElement | null>(null)
  const guard = useRef(false)
  useEffect(() => {
    if (!realFilmsAvailable) return
    let active = true; let stop: (() => void) | undefined
    void listen<string>('window-close-confirm', event => {
      if (!active || event.payload !== 'tray_unavailable') return
      setMessage('')
      setOpen(true)
    }).then(unlisten => { if (active) stop = unlisten; else unlisten() })
    return () => { active = false; stop?.() }
  }, [])
  useEffect(() => {
    if (open) { previous.current = document.activeElement as HTMLElement; dialog.current?.showModal(); cancel.current?.focus() }
    else { dialog.current?.close(); previous.current?.focus() }
  }, [open])
  const exit = async () => {
    if (guard.current) return
    guard.current = true; setBusy(true); setMessage('')
    try {
      await invoke<void>('resolve_window_close', { action: 'exit' })
    }
    catch { setMessage('退出失败，请重试。') }
    finally { guard.current = false; setBusy(false) }
  }
  return <dialog ref={dialog} className="settings-restore-dialog" aria-labelledby="exit-dialog-title" onCancel={event => { if (busy) event.preventDefault(); else setOpen(false) }} onClose={() => setOpen(false)}>
    <h3 id="exit-dialog-title">{t("托盘不可用，退出程序？")}</h3>{message && <p role="status">{t(message)}</p>}
    <div className="settings-restore-actions"><button ref={cancel} type="button" className="settings-action" disabled={busy} onClick={() => setOpen(false)}>{t("取消")}</button><button type="button" className="settings-primary" disabled={busy} onClick={() => void exit()}>{busy ? t("正在退出…") : t("退出")}</button></div>
  </dialog>
}
