import { t, useLocale } from '../i18n/react'
import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { isTauri } from '@tauri-apps/api/core'
import { invoke } from '../data/diagnostics.ts'
import { useModalCaptionHost } from './useModalCaptionHost'
import archiveMark from '../assets/archive-logo-gold.png'
import '../window-titlebar.css'

export function WindowTitleBar({ player = false }: { player?: boolean }) {
  useLocale()
  const native = isTauri()
  const modalHost = useModalCaptionHost(native)
  const [maximized, setMaximized] = useState(false)
  const [error, setError] = useState('')
  const act = async (action: string) => {
    try { setMaximized(await invoke<boolean>('window_titlebar_action', { action })); setError('') }
    catch { setError('窗口操作未完成，请重试。') }
  }
  useEffect(() => {
    if (!isTauri()) return
    const refresh = () => { void act('state') }
    refresh(); window.addEventListener('resize', refresh)
    return () => window.removeEventListener('resize', refresh)
  }, [])
  if (!native) return null
  if (!player && modalHost?.classList.contains('player-shell') && !modalHost.classList.contains('player-standalone')) return null
  const caption = <header className={`window-titlebar${player ? ' player-window-titlebar' : ''}`}>
    <div className="window-caption" onMouseDown={event => {
      event.preventDefault()
      if (event.button === 0 && event.detail === 1) void act('drag')
    }} onDoubleClick={() => { void act('maximize') }}>
      {!player && <><img className="window-emblem" src={archiveMark} alt="" draggable={false} />
      <span className="window-brand">THE ARCHIVE</span><span className="window-subtitle">{t("私人影像收藏室")}</span></>}
    </div>
    {error && <span className="window-error" role="alert">{t(error)}</span>}
    <div className="window-actions" role="group" aria-label={t("窗口控制")}>
      <button type="button" aria-label={t("最小化窗口")} title={t("最小化")} onClick={() => { void act('minimize') }}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8h10" /></svg></button>
      <button type="button" aria-label={maximized ? t("还原窗口") : t("最大化窗口")} title={maximized ? t("还原") : t("最大化")} onClick={() => { void act('maximize') }}><svg viewBox="0 0 16 16" aria-hidden="true">{maximized ? <><path d="M5 5V3h8v8h-2" /><path d="M3 5h8v8H3z" /></> : <path d="M3 3h10v10H3z" />}</svg></button>
      <button type="button" className="window-close" aria-label={t("关闭窗口")} title={t("关闭")} onClick={() => { void act('close') }}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8" /></svg></button>
    </div>
  </header>
  return modalHost ? createPortal(caption, modalHost) : caption
}
