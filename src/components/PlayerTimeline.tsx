import { t, useLocale } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import { playerControl, type PlayerState } from '../data/embeddedPlayer'
import { playerTime } from '../data/playerState'
import { playbackError } from '../data/playback'

// Remounted for each session token, so a replaced player's replies cannot move
// the next video's timeline. Native range geometry and keyboard behavior stay intact.
export function PlayerTimeline({ view, refresh, onError }: { view: PlayerState; refresh: () => Promise<void>; onError: (error: string) => void }) {
  useLocale()
  const [seek, setSeek] = useState<{ id: number; value: number; accepted: boolean; submitted: boolean } | null>(null)
  const serial = useRef(0)
  const active = useRef(true)
  useEffect(() => { active.current = true; return () => { active.current = false } }, [])
  useEffect(() => {
    if (view.stopped || view.error || view.loading) { serial.current++; setSeek(null) }
  }, [view.stopped, view.error, view.loading])
  useEffect(() => {
    // mpv command acceptance precedes decoding. Keep the target through stale
    // polls and release it only when the engine has reached it and finished seeking.
    if (seek?.accepted && !view.seeking && Math.abs(view.position - seek.value) <= Math.max(1, view.speed)) setSeek(null)
  }, [seek, view.seeking, view.position, view.speed])
  useEffect(() => {
    if (!seek?.submitted) return
    // Unseekable media or a lost response must not leave a fictitious time forever.
    const timer = window.setTimeout(() => setSeek(current => current?.id === seek.id ? null : current), 15000)
    return () => window.clearTimeout(timer)
  }, [seek?.id, seek?.submitted])
  useEffect(() => {
    if (!seek?.accepted || view.seeking) return
    // A later native +/-20s action can supersede this target before a poll
    // confirms it. Once the engine is idle, resume its actual clock promptly.
    // The short grace period allows a just-accepted seek to start decoding.
    const timer = window.setTimeout(() => setSeek(current => current?.id === seek.id ? null : current), 1000)
    return () => window.clearTimeout(timer)
  }, [seek?.id, seek?.accepted, view.seeking])
  const cancel = () => { serial.current++; setSeek(null) }
  const commit = async (value: number) => {
    const id = ++serial.current
    setSeek({ id, value, accepted: false, submitted: true })
    try {
      await playerControl(view.token, 'seek', String(value))
      if (!active.current || serial.current !== id) return
      setSeek(current => current?.id === id ? { ...current, accepted: true } : current)
      onError('')
      await refresh()
    } catch (error) {
      if (active.current && serial.current === id) { setSeek(null); onError(playbackError(error)) }
    }
  }
  const position = seek?.value ?? Math.min(view.position, view.duration || 1)
  return <div className="player-timeline">
    <input type="range" aria-label={t("播放进度")} aria-busy={Boolean(seek?.submitted)} min="0" max={view.duration || 1} step="0.1" value={position} disabled={!view.duration || view.stopped}
      onChange={event => setSeek({ id: ++serial.current, value: Number(event.target.value), accepted: false, submitted: false })}
      onPointerDown={event => { if (event.button === 0) event.currentTarget.setPointerCapture(event.pointerId) }}
      onPointerUp={event => { if (event.button === 0) void commit(Number(event.currentTarget.value)) }}
      onPointerCancel={cancel}
      onBlur={() => { if (seek && !seek.submitted) cancel() }}
      onKeyUp={event => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'].includes(event.key)) void commit(Number(event.currentTarget.value)) }} />
    <span>{playerTime(position, view.duration)} / {playerTime(view.duration)}</span>
  </div>
}
