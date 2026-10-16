import { useCallback, useLayoutEffect, useRef } from 'react'

/** Page positions belong to this app session, alongside the retained library filters. */
export function usePageScroll(page: string) {
  const positions = useRef(new Map<string, { left: number; top: number }>())
  const currentPage = useRef(page)
  currentPage.current = page
  const remember = useCallback(() => {
    positions.current.set(currentPage.current, { left: window.scrollX, top: window.scrollY })
  }, [])

  useLayoutEffect(() => {
    const position = positions.current.get(page) ?? { left: 0, top: 0 }
    window.scrollTo({ ...position, behavior: 'instant' })
    // Finish after the browser has adjusted anchoring for the newly visible page.
    const frame = requestAnimationFrame(() => window.scrollTo({ ...position, behavior: 'instant' }))
    window.addEventListener('scroll', remember, { passive: true })
    return () => { cancelAnimationFrame(frame); window.removeEventListener('scroll', remember) }
  }, [page, remember])

  // Capture before changing the visible DOM, which may clamp the document's scroll.
  return remember
}
