import { useEffect, useState, type RefObject } from 'react'
import { isProblemSelectionSurface } from '../data/problemSelection'
import { createPressFeedback } from '../data/motion'

export function useMotionEnabled(reduceMotion: boolean) {
  const [systemReduced, setSystemReduced] = useState(() => window.matchMedia('(prefers-reduced-motion: reduce)').matches)
  useEffect(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)')
    const change = () => setSystemReduced(media.matches)
    change(); media.addEventListener('change', change)
    return () => media.removeEventListener('change', change)
  }, [])
  return !reduceMotion && !systemReduced
}

export function useButtonPress(root: RefObject<HTMLDivElement | null>, enabled: boolean) {
  useEffect(() => {
    const element = root.current
    if (!element) return
    const feedback = createPressFeedback()
    const press = (event: MouseEvent) => {
      const button = event.target instanceof Element ? event.target.closest('button') : null
      const row = event.target instanceof Element ? event.target.closest<HTMLElement>('[data-app-press-surface]') : null
      if (row && element.contains(row) && isProblemSelectionSurface(event.target)) { feedback.press(row, { motion: enabled }); return }
      if (!(button instanceof HTMLButtonElement) || button.disabled || button.matches('.flip-card,.detail-backdrop')) return
      if (!element.contains(button) && !button.closest('[data-app-button-surface]')) return
      feedback.press(button, { motion: enabled, preserveFocus: button.matches(':focus-visible') })
    }
    const clear = () => feedback.clear()
    const visibility = () => { if (document.hidden) clear() }
    document.addEventListener('click', press, true)
    window.addEventListener('blur', clear)
    document.addEventListener('visibilitychange', visibility)
    return () => {
      clear(); document.removeEventListener('click', press, true)
      window.removeEventListener('blur', clear)
      document.removeEventListener('visibilitychange', visibility)
    }
  }, [root, enabled])
}
