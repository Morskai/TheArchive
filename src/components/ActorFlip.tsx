import { t, useLocale } from '../i18n/react'
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent } from 'react'
import { ActorFlipCard } from './ActorFlipCard'
import { LibraryEmptyState } from './LibraryEmptyState'
import { useMotionEnabled } from './useAppMotion'
import type { Actor } from '../data/actors'
import type { Film } from '../data/films'
import { actorFilms, averageActorRating, drawActors, getDrawPool } from '../data/actorMetrics'
export { getDrawPool } from '../data/actorMetrics'
import '../actor-flip.css'

interface ActorFlipProps {
  entranceSession: { played: boolean }
  active: boolean
  reduceMotion: boolean
  actors: Actor[]
  films: Film[]
  loading: boolean
  error?: string
  onOpenActor: (id: string, animate: boolean) => void
  onOpenActorLibrary: () => void
}

export function ActorFlip({ entranceSession, active, reduceMotion, actors, films, loading, error, onOpenActor, onOpenActorLibrary }: ActorFlipProps) {
  useLocale()
  const eligible = useMemo(() => getDrawPool(actors), [actors])
  const poolKey = eligible.map(actor => actor.id).join('|')
  const [round, setRound] = useState(() => ({ poolKey, ids: drawActors(eligible), number: 1 }))
  const [revealed, setRevealed] = useState<string[]>([])
  const page = useRef<HTMLElement>(null)
  const heading = useRef<HTMLDivElement>(null)
  const introduction = useRef<HTMLParagraphElement>(null)
  const roundBar = useRef<HTMLDivElement>(null)
  const grid = useRef<HTMLDivElement>(null)
  const instruction = useRef<HTMLParagraphElement>(null)
  const [cardWidth, setCardWidth] = useState<number | null>(null)
  const [dealRequest, setDealRequest] = useState(0)
  const [shuffling, setShuffling] = useState(false)
  const [reveals, setReveals] = useState<Record<string, { delay: number; single: boolean; animate: boolean }>>({})
  const motionEnabled = useMotionEnabled(reduceMotion)
  const pendingShuffle = useRef<(() => void) | null>(null)
  const activeRef = useRef(active)
  activeRef.current = active
  const skipDeal = useRef(false)
  const lastDeal = useRef(-1)
  const dealAnimations = useRef<Animation[]>([])

  const cancelMovement = useCallback(() => {
    dealAnimations.current.forEach(animation => { animation.onfinish = null; animation.cancel() })
    dealAnimations.current = []
    grid.current?.querySelectorAll<HTMLElement>('.flip-card').forEach(card => { card.style.zIndex = '' })
  }, [])
  const finishDeal = useCallback(() => {
    pendingShuffle.current?.()
    dealAnimations.current.filter(animation => animation.playState === 'running').forEach(animation => animation.finish())
  }, [])

  useEffect(() => {
    if (round.poolKey === poolKey) return
    pendingShuffle.current = null; cancelMovement(); setShuffling(false)
    setRound({ poolKey, ids: drawActors(eligible), number: 1 })
    setRevealed([])
    setReveals({})
  }, [eligible, poolKey, round.poolKey, cancelMovement])

  const roundActors = round.ids.flatMap(id => {
    const actor = eligible.find(item => item.id === id)
    return actor ? [actor] : []
  })
  const revealedIds = new Set(revealed)
  const revealedCount = roundActors.filter(actor => revealedIds.has(actor.id)).length

  useLayoutEffect(() => {
    const measure = () => {
      if (!page.current || !heading.current || page.current.clientWidth === 0) return
      const styles = getComputedStyle(page.current)
      const width = (page.current.clientWidth - 3 * parseFloat(styles.getPropertyValue('--flip-column-gap'))) / 4
      const foot = instruction.current ? instruction.current.offsetHeight + parseFloat(getComputedStyle(instruction.current).marginTop) : 0
      // Measure the normal layout even while its status and instruction are hidden.
      const gridTop = grid.current?.getBoundingClientRect().top ?? heading.current.getBoundingClientRect().bottom + (roundActors.length ? 0 : introduction.current?.offsetHeight ?? 0) + (roundBar.current?.offsetHeight ?? 0)
      const height = window.innerHeight - gridTop - foot - parseFloat(styles.paddingBottom)
      const next = Math.max(0, Math.min(width, Math.max(80, (height - parseFloat(styles.getPropertyValue('--flip-row-gap'))) / 2) * 5 / 7))
      setCardWidth(current => current !== null && Math.abs(current - next) < 0.1 ? current : next)
    }
    measure()
    const observer = new ResizeObserver(measure)
    if (page.current) observer.observe(page.current)
    window.addEventListener('resize', measure)
    return () => { observer.disconnect(); window.removeEventListener('resize', measure) }
  }, [active, roundActors.length, loading, error])

  useLayoutEffect(() => {
    if (!active || loading || error || !grid.current || !roundActors.length || cardWidth === null || lastDeal.current === dealRequest) return
    lastDeal.current = dealRequest
    if (dealRequest === 0 && entranceSession.played) return
    entranceSession.played = true
    if (skipDeal.current) { skipDeal.current = false; return }
    if (!motionEnabled) return
    const cards = [...grid.current.querySelectorAll<HTMLButtonElement>('.flip-card')]
    const bounds = grid.current.getBoundingClientRect()
    const centerX = bounds.left + bounds.width / 2
    const centerY = bounds.top + bounds.height / 2
    dealAnimations.current = cards.map((card, index) => {
      const box = card.getBoundingClientRect()
      const x = centerX - box.left - box.width / 2 + index * 1.5
      const y = centerY - box.top - box.height / 2 - index * 1.5
      const rotation = (index - (cards.length - 1) / 2) * 2.6
      card.style.zIndex = String(cards.length - index)
      const animation = card.animate([
        { transform: `translate(${x}px, ${y}px) rotate(${rotation}deg) scale(.88)`, offset: 0 },
        { transform: `translate(${-x * .045}px, ${-y * .045}px) rotate(${-rotation * .4}deg) scale(1.015)`, offset: .8 },
        { transform: 'none', offset: 1 },
      ], { duration: dealRequest === 0 ? 500 : 300, delay: (dealRequest === 0 ? 180 : 40) + index * 35, easing: getComputedStyle(card).getPropertyValue('--ease-out').trim(), fill: 'backwards' })
      animation.onfinish = () => { card.style.zIndex = ''; animation.cancel() }
      return animation
    })
    return () => { pendingShuffle.current?.(); cancelMovement() }
  }, [active, loading, error, cardWidth, roundActors.length, dealRequest, motionEnabled, entranceSession, cancelMovement])

  useLayoutEffect(() => {
    if (!active || !motionEnabled) finishDeal()
    const hidden = () => { if (document.hidden) finishDeal() }
    window.addEventListener('blur', finishDeal)
    document.addEventListener('visibilitychange', hidden)
    return () => { window.removeEventListener('blur', finishDeal); document.removeEventListener('visibilitychange', hidden) }
  }, [active, motionEnabled, finishDeal])
  useEffect(() => () => { pendingShuffle.current = null; cancelMovement() }, [cancelMovement])

  const shuffleRound = (event: MouseEvent<HTMLButtonElement>) => {
    if (pendingShuffle.current) { pendingShuffle.current(); return }
    finishDeal()
    const next = drawActors(eligible)
    if (next.length > 1 && next.every((id, index) => id === round.ids[index])) {
      ;[next[0], next[1]] = [next[1], next[0]]
    }
    const animate = motionEnabled && event.detail > 0
    const commit = () => {
      if (pendingShuffle.current !== commit) return
      pendingShuffle.current = null
      skipDeal.current = !activeRef.current || !animate
      setRound({ poolKey, ids: next, number: round.number + 1 })
      setRevealed([]); setReveals({}); setShuffling(false)
      setDealRequest(current => current + 1)
    }
    pendingShuffle.current = commit
    if (!animate || !grid.current) { commit(); return }
    setShuffling(true)
    const cards = [...grid.current.querySelectorAll<HTMLButtonElement>('.flip-card')]
    const bounds = grid.current.getBoundingClientRect()
    dealAnimations.current = cards.map((card, index) => {
      const box = card.getBoundingClientRect()
      const x = bounds.left + bounds.width / 2 - box.left - box.width / 2
      const y = bounds.top + bounds.height / 2 - box.top - box.height / 2
      card.style.zIndex = String(index + 1)
      return card.animate([
        { transform: 'none' },
        { transform: `translate(${x}px,${y}px) rotate(${(index - 3.5) * 2}deg) scale(.88)` },
      ], { duration: 240, delay: (cards.length - index - 1) * 12, easing: getComputedStyle(card).getPropertyValue('--ease-in-out').trim(), fill: 'forwards' })
    })
    dealAnimations.current[0].onfinish = commit
  }
  const revealCard = (id: string, animate: boolean) => {
    setReveals(current => ({ ...current, [id]: { delay: 0, single: true, animate } }))
    setRevealed(current => current.includes(id) ? current : [...current, id])
  }
  const revealRemaining = (event: MouseEvent<HTMLButtonElement>) => {
    finishDeal()
    const remaining = round.ids.filter(id => !revealedIds.has(id))
    setReveals(current => ({ ...current, ...Object.fromEntries(remaining.map((id, index) => [id, { delay: index * 45, single: false, animate: event.detail > 0 }])) }))
    setRevealed(round.ids)
  }

  return <section ref={page} className="actor-flip-page" data-motion={motionEnabled} aria-labelledby="actor-flip-title">
    <div ref={heading} className="flip-heading">
      <div><h1 id="actor-flip-title">{t("演员翻牌")}</h1><p ref={introduction} className={roundActors.length ? undefined : 'flip-measure-only'} aria-hidden={roundActors.length ? undefined : true}>{t("选一张牌，看看下一位遇见谁。")}</p></div>
      <div className="flip-actions"><button type="button" onClick={shuffleRound} disabled={loading || Boolean(error) || roundActors.length === 0}>{t("重新洗牌")}</button><button type="button" className="settings-primary" onClick={revealRemaining} disabled={loading || Boolean(error) || shuffling || roundActors.length === 0 || revealedCount === roundActors.length}>{t("翻开剩余")}</button></div>
    </div>

    <div ref={roundBar} className={`flip-round-bar${loading || error || !roundActors.length ? ' flip-measure-only' : ''}`} aria-hidden={loading || error || !roundActors.length ? true : undefined}><div><span className="flip-round-label">{t("本轮")}</span><strong>{String(round.number).padStart(2, '0')}</strong><span className="flip-round-separator">/</span><span>{t("{0}位演员", [roundActors.length])}</span></div><div className="flip-progress"><span>{t("已翻开")}<strong>{revealedCount}</strong> / {roundActors.length}</span><div className="flip-progress-track" role="progressbar" aria-label={t("本轮翻牌进度")} aria-valuemin={0} aria-valuemax={roundActors.length} aria-valuenow={revealedCount}><i style={{ transform: `scaleX(${roundActors.length ? revealedCount / roundActors.length : 0})` }} /></div></div></div>
    {loading ? <div className="flip-empty" role="status">{t("正在准备卡片…")}</div> : error ? <div className="flip-empty" role="alert">{t(error)}</div> : roundActors.length === 0 ? <LibraryEmptyState kind="flip" title={t("暂无可翻牌演员")} maxCardWidth={cardWidth ?? 0}><button className="button button-gold" type="button" onClick={onOpenActorLibrary}>{t("前往演员库")}</button></LibraryEmptyState> : <>
      <div ref={grid} className={`flip-grid${shuffling ? ' is-shuffling' : ''}`} style={cardWidth === null ? undefined : { '--flip-card-width': `${cardWidth}px` } as CSSProperties}>{roundActors.map((actor, index) => {
        const isRevealed = revealedIds.has(actor.id)
        const averageRating = averageActorRating(actorFilms(actor.id, films))
        return <ActorFlipCard key={`${round.number}-${actor.id}`} actor={actor} index={index} rating={averageRating} revealed={isRevealed}
          revealDelay={reveals[actor.id]?.delay ?? 0} singleReveal={reveals[actor.id]?.single ?? false} active={active && !shuffling}
          disabled={shuffling} motion={motionEnabled} animateReveal={motionEnabled && (reveals[actor.id]?.animate ?? true)} onReveal={revealCard} onOpen={onOpenActor} onFinishDeal={finishDeal} />
      })}</div>
    </>}
    <p ref={instruction} className={`flip-instruction${loading || error || !roundActors.length ? ' flip-measure-only' : ''}`} aria-hidden={loading || error || !roundActors.length ? true : undefined}>{t("翻开卡片后，再次点击可查看演员详情。")}</p>
  </section>
}
