import { t, useLocale } from '../i18n/react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { isTauri } from '@tauri-apps/api/core'
import { invoke } from '../data/diagnostics'
import { listen } from '@tauri-apps/api/event'
import { readPlayer, playerControl, closePlayer, layoutPlayer, choosePlayerSubtitle, type PlayerState } from '../data/embeddedPlayer'
import { adjacentVideoId, createPlayerReader, playerKeyAction } from '../data/playerState'
import { PlayerTimeline } from './PlayerTimeline'
import { launchFilmVideo, notifyRealLibraryChanged, playbackError } from '../data/playback'
import { loadInterfacePreferences } from '../data/settings'
import { useMotionEnabled } from './useAppMotion'
import { useModalCaptionHost } from './useModalCaptionHost'
import '../embedded-player.css'

type IconName = 'back' | 'stop' | 'previous' | 'play' | 'pause' | 'next' | 'volume' | 'muted' | 'subtitle' | 'audio' | 'episodes' | 'aspect' | 'speed' | 'fullscreen' | 'restore'
function PlayerIcon({ name }: { name: IconName }) {
  const paths: Record<IconName, React.ReactNode> = {
    back: <path d="m14 5-7 7 7 7M7 12h14" />,
    stop: <rect x="4" y="4" width="16" height="16" rx=".7" fill="currentColor" stroke="none" />,
    previous: <><path d="M5 4v16" /><path d="m20 4-12 8 12 8Z" fill="currentColor" stroke="none" /></>,
    next: <><path d="M19 4v16" /><path d="m4 4 12 8-12 8Z" fill="currentColor" stroke="none" /></>,
    play: <path d="m6 3 15 9-15 9Z" fill="currentColor" stroke="none" />,
    pause: <><path d="M7 4v16M17 4v16" strokeWidth="5" /></>,
    volume: <><path d="M4 9h4l5-4v14l-5-4H4Z" /><path d="M16 8c3 2 3 6 0 8M19 5c5 4 5 10 0 14" /></>,
    muted: <><path d="M4 9h4l5-4v14l-5-4H4Z" /><path d="m17 9 5 6m0-6-5 6" /></>,
    subtitle: <><rect x="2" y="4" width="20" height="16" rx="1" /><path d="M5 10h5m3 0h6M5 15h3m3 0h8" /></>,
    audio: <><path d="M4 14v-3a8 8 0 0 1 16 0v3" /><rect x="3" y="12" width="4" height="8" rx="1" /><rect x="17" y="12" width="4" height="8" rx="1" /></>,
    episodes: <><path d="m3 4 6 4-6 4Z" /><path d="M13 5h9M13 10h9M3 16h19M3 21h19" /></>,
    aspect: <><rect x="2" y="5" width="20" height="14" rx="1" /><path d="M6 9h3m-3 0v3m12 3h-3m3 0v-3" /></>,
    speed: <><circle cx="12" cy="12" r="9.5" /><text x="12" y="15.5" textAnchor="middle" fill="currentColor" stroke="none" fontSize="10" fontWeight="600" fontFamily="inherit">1×</text></>,
    fullscreen: <path d="M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5" />,
    restore: <path d="M3 8h5V3m8 0v5h5M8 21v-5H3m13 5v-5h5" />,
  }
  return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>
}

export function EmbeddedPlayerHost({ standalone = false, reduceMotion = false }: { standalone?: boolean; reduceMotion?: boolean }) {
  useLocale()
  const [view, setView] = useState<PlayerState | null>(null)
  const [savedReduce, setSavedReduce] = useState(reduceMotion)
  const [error, setError] = useState('')
  const currentWindow = standalone ? 'player' : 'main'
  const active = useRef(true)
  const refresh = useRef(createPlayerReader(readPlayer, (value: PlayerState | null) => {
    if (active.current) { setView(value?.window === currentWindow ? value : null); setError('') }
  })).current
  useEffect(() => {
    if (!isTauri()) return
    active.current = true
    let disposed = false
    const unlisten: (() => void)[] = []
    const subscribe = async () => {
      for (const [name, handler] of [
        ['embedded-player-opening', (event: { payload: PlayerState }) => {
          if (disposed) return
          refresh.invalidate()
          const value = event.payload
          setView(current => current && current.token > value.token ? current : value.window === currentWindow ? value : null)
          setError('')
        }],
        ['embedded-player-changed', () => { void refresh().catch(() => setError('播放状态暂时无法读取。')); if (!standalone) notifyRealLibraryChanged() }],
      ] as const) {
        const stop = await listen<PlayerState>(name, handler)
        if (disposed) stop(); else unlisten.push(stop)
      }
      if (!disposed) void refresh().catch(() => setError('播放状态暂时无法读取。'))
    }
    void subscribe()
      .catch(() => setError('播放器通知无法读取，请重新打开播放器。'))
    if (standalone) void loadInterfacePreferences().then(value => { if (active.current) setSavedReduce(value.reduceMotion) }).catch(() => {})
    return () => { disposed = true; active.current = false; unlisten.forEach(stop => stop()) }
  }, [refresh, standalone])
  useEffect(() => {
    if (!view) return
    let running = false
    const timer = window.setInterval(() => {
      if (running) return
      running = true
      void refresh().catch(() => setError('播放状态暂时无法读取。')).finally(() => { running = false })
    }, 500)
    return () => window.clearInterval(timer)
  }, [Boolean(view), refresh])
  if (!view) return standalone ? <div className="player-idle" role="status">{t(error) || t("播放器已关闭")}</div> : null
  return <EmbeddedPlayer view={view} reduceMotion={standalone ? savedReduce : reduceMotion} standalone={standalone} refresh={refresh} readError={t(error)} />
}

function EmbeddedPlayer({ view, reduceMotion, standalone, refresh, readError }: { view: PlayerState; reduceMotion: boolean; standalone: boolean; refresh: () => Promise<void>; readError: string }) {
  useLocale()
  const dialog = useRef<HTMLDialogElement>(null)
  const stage = useRef<HTMLDivElement>(null)
  const volumePopup = useRef<HTMLDivElement>(null)
  const [volumeOpen, setVolumeOpen] = useState(false)
  const [ready, setReady] = useState(standalone)
  const [exiting, setExiting] = useState(false)
  const [panel, setPanel] = useState<'episodes' | 'subtitle' | 'audio' | 'aspect' | 'speed' | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const motion = useMotionEnabled(reduceMotion)
  const topModal = useModalCaptionHost(true)
  const token = useRef(view.token)
  token.current = view.token
  const lastVolume = useRef(100)

  useEffect(() => { setError(''); setExiting(false) }, [view.token])
  useEffect(() => {
    const node = dialog.current
    if (!node) return
    const focus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const scroll = { left: window.scrollX, top: window.scrollY }
    document.documentElement.classList.add('player-active')
    document.body.classList.add('player-active')
    node.showModal()
    stage.current?.focus({ preventScroll: true })
    let animation: Animation | undefined
    if (!standalone && motion) {
      animation = node.animate([{ transform: 'translateX(100%)' }, { transform: 'translateX(0)' }], { duration: 280, easing: 'cubic-bezier(.32,.72,0,1)' })
      void animation.finished.then(() => setReady(true)).catch(() => {})
    } else setReady(true)
    return () => { animation?.cancel(); node.close(); document.documentElement.classList.remove('player-active'); document.body.classList.remove('player-active'); window.scrollTo(scroll); focus?.focus({ preventScroll: true }) }
    // Opening motion runs once; selecting another episode reuses this presentation.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  useEffect(() => { if (!motion) { dialog.current?.getAnimations().forEach(a => a.finish()); setReady(true) } }, [motion])
  useEffect(() => {
    const node = stage.current
    if (!node) return
    let pending = false, again = false, disposed = false
    const update = () => {
      if (pending) { again = true; return }
      pending = true
      void layoutPlayer(view.token, node, ready && !view.loading && !exiting && !document.hidden && !view.stopped && !view.error && topModal === dialog.current, volumeOpen ? volumePopup.current : null).catch(error => {
        if (!disposed && String(error) !== 'player_replaced') setError(playbackError(error))
      }).finally(() => { pending = false; if (!disposed && again) { again = false; update() } })
    }
    const observer = new ResizeObserver(update)
    observer.observe(node)
    window.addEventListener('resize', update)
    document.addEventListener('visibilitychange', update)
    update()
    return () => { disposed = true; observer.disconnect(); window.removeEventListener('resize', update); document.removeEventListener('visibilitychange', update) }
  }, [view.token, ready, view.loading, exiting, panel, volumeOpen, view.stopped, view.error, topModal])

  const run = useCallback(async (action: string, value?: string) => {
    const current = view.token
    try { await playerControl(current, action, value); if (token.current === current) { setError(''); await refresh() } }
    catch (error) { if (token.current === current) setError(playbackError(error)) }
  }, [view.token, refresh])
  const dismiss = useCallback(async () => {
    if (exiting) return
    const current = view.token
    setExiting(true)
    try {
      // Initialization can fail before there is an engine or native surface.
      // Closing the token-owned session must still restore the library.
      if (!view.error) {
        if (stage.current) await layoutPlayer(current, stage.current, false)
        await playerControl(current, 'stop')
      }
      if (!standalone && motion && dialog.current) await dialog.current.animate([{ transform: 'translateX(0)' }, { transform: 'translateX(100%)' }], { duration: 200, easing: 'cubic-bezier(.32,.72,0,1)', fill: 'forwards' }).finished
      await closePlayer(current)
      await refresh()
    } catch (error) { if (token.current === current) { setExiting(false); dialog.current?.getAnimations().forEach(a => a.cancel()); setError(playbackError(error)) } }
  }, [view.token, view.error, exiting, standalone, motion, refresh])
  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen<number>('embedded-player-escape', event => {
      if (disposed || event.payload !== view.token || topModal !== dialog.current) return
      if (panel) setPanel(null); else void dismiss()
    }).then(stop => { if (disposed) stop(); else unlisten = stop }).catch(() => {})
    return () => { disposed = true; unlisten?.() }
  }, [view.token, panel, topModal, dismiss])
  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    void listen<number>('embedded-player-back', event => {
      if (!disposed && event.payload === view.token && topModal === dialog.current) void dismiss()
    }).then(stop => { if (disposed) stop(); else unlisten = stop }).catch(() => {})
    return () => { disposed = true; unlisten?.() }
  }, [view.token, topModal, dismiss])
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (topModal !== dialog.current || event.altKey || event.ctrlKey || event.metaKey) return
      const target = event.target instanceof Element ? event.target : null
      if (event.code === 'Backspace' && !target?.closest('input:not([type=range]),textarea,[contenteditable=true]')) { event.preventDefault(); void dismiss(); return }
      if (target?.closest('button,input,select,textarea,a,[contenteditable=true]') || view.loading) return
      const action = playerKeyAction(event.code)
      if (action) { event.preventDefault(); void run(action.action, action.value) }
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [run, topModal, dismiss, view.loading])

  const chooseVideo = async (videoId: string) => {
    if (busy) return
    setBusy(true)
    const current = view.token
    try { const result = await launchFilmVideo(view.filmId, videoId, current); if (!result.historySaved) setError('视频已打开，播放记录未保存。'); notifyRealLibraryChanged(); await refresh() }
    catch (error) { setError(playbackError(error)); await refresh().catch(() => {}) }
    finally { setBusy(false) }
  }
  const selectPanel = (value: typeof panel) => setPanel(current => current === value ? null : value)
  const selectedName = view.videos.find(video => video.id === view.videoId)?.name ?? ''
  const tracks = view.tracks.filter(track => track.kind === (panel === 'audio' ? 'audio' : 'sub'))
  const notice = error || readError || (view.error ? playbackError(view.error) : '')
  const previous = adjacentVideoId(view.videos, view.videoId, -1)
  const next = adjacentVideoId(view.videos, view.videoId, 1)
  return createPortal(<dialog ref={dialog} className={`player-shell${standalone ? ' player-standalone' : ''}`} aria-labelledby="player-title" onCancel={event => { event.preventDefault(); if (panel) setPanel(null); else void dismiss() }}>
    <div className="player-top" onMouseDown={event => {
      if (standalone || event.button !== 0 || event.detail !== 1 || view.fullscreen || (event.target instanceof Element && event.target.closest('button'))) return
      event.preventDefault()
      void invoke('window_titlebar_action', { action: 'drag' }).catch(() => setError('窗口未能移动，请重试。'))
    }}>{!standalone && <button className="player-back" aria-label={t("关闭播放器")} title={t("返回片库（Backspace）")} disabled={exiting} onClick={() => { void dismiss() }}><PlayerIcon name="back" /></button>}<div className="player-heading"><span>{t("正在播放")}</span><h1 id="player-title" title={view.title}>{view.title}</h1><p title={selectedName}>{selectedName}</p></div></div>
    <div className={`player-body${panel ? ' player-panel-open' : ''}`}>
      <div ref={stage} className="player-stage" aria-label={t("视频画面")} tabIndex={-1} onDoubleClick={() => { if (!view.loading) void run('pause') }}><div className="player-stage-message" role="status">{view.stopped ? t("已停止") : !ready || view.loading ? t("正在打开视频…") : ''}</div></div>
      {panel && <aside className="player-panel" aria-label={{ episodes: t("选集"), subtitle: t("字幕"), audio: t("音轨"), aspect: t("画面比例"), speed: t("播放速度") }[panel]}>
        <div className="player-panel-heading"><h2>{{ episodes: t("选集"), subtitle: t("字幕"), audio: t("音轨"), aspect: t("画面比例"), speed: t("播放速度") }[panel]}</h2><button aria-label={t("收起播放选项")} onClick={() => setPanel(null)}>×</button></div>
        <div className="player-panel-list">
          {panel === 'episodes' && view.videos.map((video, index) => <button key={video.id} disabled={busy} aria-pressed={video.id === view.videoId} onClick={() => { void chooseVideo(video.id) }} title={video.name}><span className="player-index">{String(index + 1).padStart(2, '0')}</span><span>{video.name}</span></button>)}
          {(panel === 'subtitle' || panel === 'audio') && <>
            {panel === 'subtitle' && <button onClick={() => { void choosePlayerSubtitle(view.token).then(refresh).catch(error => setError(playbackError(error))) }}>{t("选择外挂字幕…")}</button>}
            <button aria-pressed={!tracks.some(track => track.selected)} onClick={() => { void run(panel, 'no') }}>{panel === 'subtitle' ? t("关闭字幕") : t("关闭音轨")}</button>
            {tracks.map(track => <button key={track.id} aria-pressed={track.selected} title={track.title} onClick={() => { void run(panel, track.id) }}>{track.title}</button>)}
            {!tracks.length && <p className="player-panel-empty">{panel === 'subtitle' ? t("没有可选字幕，可选择外挂字幕。") : t("没有可选音轨。")}</p>}
          </>}
          {panel === 'aspect' && [['original', t("原始比例")], ['16:9', '16 : 9'], ['4:3', '4 : 3'], ['stretch', t("拉伸铺满")]].map(([value, name]) => <button key={value} aria-pressed={view.aspect === value} onClick={() => { void run('aspect', value) }}>{name}</button>)}
          {panel === 'speed' && [0.5, 0.75, 1, 1.25, 1.5, 2].map(value => <button key={value} aria-pressed={view.speed === value} onClick={() => { void run('speed', String(value)) }}>{value}×</button>)}
        </div>
      </aside>}
    </div>
    <footer className="player-controls" inert={view.loading} aria-busy={view.loading}>
      <div className="player-notice" role="status">{t(notice)}</div>
      <PlayerTimeline key={view.token} view={view} refresh={refresh} onError={setError} />
      <div className="player-toolbar">
        <div className="player-transport">
          <button className="player-stop" aria-label={t("停止")} title={t("停止")} disabled={view.stopped} onClick={() => { void run('stop') }}><PlayerIcon name="stop" /></button>
          <button aria-label={t("上一集")} title={t("上一集")} disabled={!previous || busy} onClick={() => { if (previous) void chooseVideo(previous) }}><PlayerIcon name="previous" /></button>
          <button className="player-toggle" aria-label={view.paused || view.stopped ? t("播放") : t("暂停")} title={t("播放／暂停（空格、双击画面）")} onClick={() => { void run('pause') }}><PlayerIcon name={view.paused || view.stopped ? 'play' : 'pause'} /></button>
          <button aria-label={t("下一集")} title={t("下一集")} disabled={!next || busy} onClick={() => { if (next) void chooseVideo(next) }}><PlayerIcon name="next" /></button>
          <div className="player-volume" onMouseEnter={() => setVolumeOpen(true)} onMouseLeave={event => { if (!event.currentTarget.contains(document.activeElement)) setVolumeOpen(false) }} onFocus={() => setVolumeOpen(true)} onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget) && !event.currentTarget.matches(':hover')) setVolumeOpen(false) }}>
            <button aria-label={view.volume ? t("静音") : t("取消静音")} title={t("音量")} aria-expanded={volumeOpen} onClick={() => { if (view.volume) lastVolume.current = view.volume; void run('volume', String(view.volume ? 0 : lastVolume.current)) }}><PlayerIcon name={view.volume ? 'volume' : 'muted'} /></button>
            {volumeOpen && <div ref={volumePopup} className="player-volume-popup"><output>{Math.round(view.volume)}%</output><input aria-label={t("音量")} aria-orientation="vertical" type="range" min="0" max="100" value={view.volume} onChange={event => { void run('volume', event.target.value) }} /></div>}
          </div>
        </div>
        <div className="player-options">{(['subtitle', 'audio', 'episodes', 'aspect', 'speed'] as const).map(value => <button key={value} aria-label={{ subtitle: t("字幕"), audio: t("音轨"), episodes: t("选集"), aspect: t("比例"), speed: t("倍速") }[value]} title={{ subtitle: t("字幕"), audio: t("音轨"), episodes: t("选集"), aspect: t("画面比例"), speed: t("倍速（{0}×）", [view.speed || 1]) }[value]} aria-expanded={panel === value} onClick={() => selectPanel(value)}><PlayerIcon name={value} /></button>)}<button aria-label={view.fullscreen ? t("取消全屏") : t("全屏")} title={t("全屏／取消全屏（回车）")} onClick={() => { void run('fullscreen') }}><PlayerIcon name={view.fullscreen ? 'restore' : 'fullscreen'} /></button></div>
      </div>
    </footer>
  </dialog>, document.body)
}
