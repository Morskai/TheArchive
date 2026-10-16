import { useEffect, useRef } from 'react'
import '../comet-trail.css'

// Decorative only: retain the system pointer and never intercept input.
export function CometTrail({ reduceMotion }: { reduceMotion: boolean }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  useEffect(() => {
    const element = canvas.current
    const context = element?.getContext('2d')
    if (!element || !context) return
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)')
    const pointer = window.matchMedia('(any-pointer: fine)')
    type Point = { x: number; y: number; born: number }
    let points: Point[] = [], frame = 0, lastEmission = 0
    let width = 0, height = 0
    const clear = () => {
      cancelAnimationFrame(frame); frame = 0; points = []
      context.clearRect(0, 0, width, height)
    }
    const enabled = () => !reduceMotion && !motion.matches && pointer.matches && !document.hidden && document.hasFocus()
    const resize = () => {
      clear()
      width = window.innerWidth; height = window.innerHeight
      const ratio = Math.min(window.devicePixelRatio || 1, 1.5)
      element.width = Math.round(width * ratio); element.height = Math.round(height * ratio)
      context.setTransform(ratio, 0, 0, ratio, 0, 0)
    }
    const draw = (now: number) => {
      frame = 0
      context.clearRect(0, 0, width, height)
      if (!enabled()) { clear(); return }
      points = points.filter(point => now - point.born < 540)
      for (let index = 1; index < points.length; index++) {
        const previous = points[index - 1], point = points[index]
        const life = 1 - (now - point.born) / 540
        context.beginPath(); context.moveTo(previous.x, previous.y); context.lineTo(point.x, point.y)
        context.lineCap = 'round'; context.lineWidth = 2.04 * life
        context.strokeStyle = `rgba(222,183,111,${life * .5})`; context.stroke()
        // Small drifting embers trace the gesture instead of obscuring content.
        if (index % 3 === 0) {
          const drift = (now - point.born) / 35
          context.beginPath(); context.arc(point.x + Math.sin(index * 2.4) * drift, point.y - drift, (.6 + life) * 1.2, 0, Math.PI * 2)
          context.fillStyle = `rgba(246,216,159,${life * .65})`; context.fill()
        }
      }
      const head = points.at(-1)
      if (head) {
        const life = Math.max(0, 1 - (now - head.born) / 220)
        const light = context.createRadialGradient(head.x, head.y, 0, head.x, head.y, 13)
        light.addColorStop(0, `rgba(255,236,194,${life * .8})`)
        light.addColorStop(.16, `rgba(234,192,120,${life * .3})`)
        light.addColorStop(1, 'rgba(234,192,120,0)')
        context.fillStyle = light; context.fillRect(head.x - 13, head.y - 13, 26, 26)
      }
      if (points.length) frame = requestAnimationFrame(draw)
    }
    const move = (event: PointerEvent) => {
      if (event.pointerType !== 'mouse' || !enabled()) return
      if (event.target instanceof Element && event.target.closest('button,input,select,textarea,[role="dialog"]')) { clear(); return }
      const now = performance.now()
      if (now - lastEmission < 12) return
      lastEmission = now
      const previous = points.at(-1)
      if (previous && Math.hypot(event.clientX - previous.x, event.clientY - previous.y) > 96) points = []
      points.push({ x: event.clientX, y: event.clientY, born: now })
      if (points.length > 36) points.shift()
      if (!frame) frame = requestAnimationFrame(draw)
    }
    resize()
    window.addEventListener('pointermove', move, { passive: true })
    window.addEventListener('resize', resize)
    window.addEventListener('blur', clear)
    document.addEventListener('pointerleave', clear)
    document.addEventListener('visibilitychange', clear)
    motion.addEventListener('change', clear); pointer.addEventListener('change', clear)
    return () => {
      clear()
      window.removeEventListener('pointermove', move); window.removeEventListener('resize', resize)
      window.removeEventListener('blur', clear); document.removeEventListener('pointerleave', clear)
      document.removeEventListener('visibilitychange', clear)
      motion.removeEventListener('change', clear); pointer.removeEventListener('change', clear)
    }
  }, [reduceMotion])
  return <canvas ref={canvas} className="comet-trail" aria-hidden="true" />
}
