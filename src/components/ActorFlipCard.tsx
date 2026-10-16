import { t, useLocale } from '../i18n/react'
import { useLayoutEffect, useRef, useState, type MouseEvent, type PointerEvent } from 'react'
import { ActorPortrait } from './ActorLibrary'
import type { Actor } from '../data/actors'

interface ActorFlipCardProps {
  actor: Actor
  index: number
  rating: number | null
  revealed: boolean
  revealDelay: number
  singleReveal: boolean
  active: boolean
  disabled: boolean
  motion: boolean
  animateReveal: boolean
  onReveal: (id: string, animate: boolean) => void
  onOpen: (id: string, animate: boolean) => void
  onFinishDeal: () => void
}

/** Separate lift, turn and light layers so feedback never retargets a half-turned face. */
export function ActorFlipCard({ actor, index, rating, revealed, revealDelay, singleReveal, active, disabled, motion, animateReveal, onReveal, onOpen, onFinishDeal }: ActorFlipCardProps) {
  useLocale()
  const button = useRef<HTMLButtonElement>(null)
  const lift = useRef<HTMLSpanElement>(null)
  const inner = useRef<HTMLSpanElement>(null)
  const reflection = useRef<HTMLSpanElement>(null)
  const turn = useRef<Animation | null>(null)
  const stopReveal = useRef<(() => void) | null>(null)
  const frame = useRef(0)
  const wasRevealed = useRef(revealed)
  const [revealing, setRevealing] = useState(false)

  const resetPointer = () => {
    cancelAnimationFrame(frame.current); frame.current = 0
    if (lift.current) lift.current.style.transform = ''
    if (reflection.current) reflection.current.style.transform = ''
  }

  useLayoutEffect(() => {
    if (!revealed || wasRevealed.current || !inner.current || !button.current) { wasRevealed.current = revealed; return }
    wasRevealed.current = true
    if (!animateReveal || !active) return
    resetPointer()
    setRevealing(true)
    const ease = getComputedStyle(button.current).getPropertyValue('--ease-out').trim()
    // Match the original turn curve so the sweep starts over the front, not the back.
    const rotate = inner.current.animate([
      { transform: 'rotateY(0deg)' },
      { transform: 'rotateY(180deg)' },
    ], { duration: 580, delay: revealDelay, easing: getComputedStyle(button.current).getPropertyValue('--ease').trim(), fill: 'backwards' })
    turn.current = rotate
    const rise = lift.current!.animate([
      { transform: 'translateY(0) scale(.985)' },
      { transform: 'translateY(-8px) scale(1.025)', offset: .4 },
      { transform: 'none' },
    ], { duration: 460, delay: revealDelay, easing: ease })
    const animations = [rotate, rise]
    const light = button.current.querySelector('.flip-reveal-light')!
    const sweep = light.animate([
      { opacity: 0, transform: 'translateX(-20%)', easing: 'ease-out' },
      { opacity: .75, offset: .28, easing: 'ease-out' },
      { opacity: 0, transform: 'translateX(24%)' },
    ], { duration: 640, delay: revealDelay + 180, easing: 'linear', fill: 'both' })
    animations.push(sweep)
    if (singleReveal) {
      button.current.querySelectorAll<HTMLElement>('.flip-edge-spark').forEach((spark, index) => {
        const direction = index % 2 ? 1 : -1
        animations.push(spark.animate([
          { opacity: 0, transform: 'translate(0,0) scale(.8)' },
          { opacity: .9, offset: .2 },
          { opacity: 0, transform: `translate(${direction * (8 + index * 2)}px,${-12 - index * 5}px) scale(.5)` },
        ], { duration: 240, delay: revealDelay + 160 + index * 12, easing: ease }))
      })
    }
    const finishTurn = () => {
      if (turn.current !== rotate) return
      turn.current = null
      rotate.onfinish = null; rotate.cancel(); rise.cancel()
      setRevealing(false)
    }
    const stop = () => {
      finishTurn()
      animations.forEach(animation => { animation.onfinish = null; animation.cancel() })
      if (stopReveal.current === stop) stopReveal.current = null
    }
    // The previous broad sweep continues across the settled portrait after the turn ends.
    rotate.onfinish = finishTurn
    sweep.onfinish = stop
    stopReveal.current = stop
    return stop
  }, [revealed, active, animateReveal, actor.id, revealDelay, singleReveal])

  useLayoutEffect(() => {
    const stop = () => {
      resetPointer()
      stopReveal.current?.()
    }
    const hidden = () => { if (document.hidden) stop() }
    window.addEventListener('blur', stop)
    document.addEventListener('visibilitychange', hidden)
    return () => { stop(); window.removeEventListener('blur', stop); document.removeEventListener('visibilitychange', hidden) }
  }, [])

  const point = (event: PointerEvent<HTMLButtonElement>) => {
    if (!motion || revealing || event.buttons || event.pointerType !== 'mouse' || !window.matchMedia('(hover: hover) and (pointer: fine)').matches) return
    const bounds = event.currentTarget.getBoundingClientRect()
    const x = Math.max(-.5, Math.min(.5, (event.clientX - bounds.left) / bounds.width - .5))
    const y = Math.max(-.5, Math.min(.5, (event.clientY - bounds.top) / bounds.height - .5))
    cancelAnimationFrame(frame.current)
    frame.current = requestAnimationFrame(() => {
      frame.current = 0
      if (lift.current) lift.current.style.transform = `translateY(-4px) rotateX(${-y * 5}deg) rotateY(${x * 5}deg)`
      if (reflection.current) reflection.current.style.transform = `translateX(${x * 35}%) rotate(-24deg)`
    })
  }
  const activate = (event: MouseEvent<HTMLButtonElement>) => {
    onFinishDeal()
    if (turn.current) { stopReveal.current?.(); return }
    if (revealed) { resetPointer(); onOpen(actor.id, motion && event.detail > 0) }
    else onReveal(actor.id, motion && event.detail > 0)
  }
  return <button ref={button} type="button" data-actor-id={actor.id}
    className={`flip-card${revealed ? ' is-revealed' : ''}${revealing ? ' is-revealing' : ''}`}
    aria-label={revealing ? t("正在翻开{0}，再次点击完成揭晓", [actor.name]) : revealed ? t("查看{0}演员详情", [actor.name]) : t("翻开第{0}张演员卡", [index + 1])}
    aria-pressed={revealed} disabled={disabled} onClick={activate} onPointerMove={point} onPointerDown={resetPointer} onPointerLeave={resetPointer}>
    <span className="flip-card-shadow" aria-hidden="true" />
    <span ref={lift} className="flip-card-lift" aria-hidden="true">
      <span ref={inner} className="flip-card-inner">
        <span className="flip-card-face flip-card-back"><span className="flip-card-index">{String(index + 1).padStart(2, '0')}</span><span className="flip-card-reflection" ref={reflection} /></span>
        <span className="flip-card-face flip-card-front"><ActorPortrait actor={actor} large /><span className="flip-card-gradient" /><span className="flip-card-front-caption"><strong title={actor.name}>{actor.name}</strong><span className="flip-card-rating">{rating === null ? t("暂无评分") : <><span className="star-shape filled" />{rating.toFixed(1)}</>}</span></span></span>
      </span>
    </span>
    <span className="flip-reveal" aria-hidden="true"><span className="flip-reveal-light" />{singleReveal && Array.from({ length: 4 }, (_, particle) => <i key={particle} className={`flip-edge-spark spark-${particle}`} />)}</span>
  </button>
}
