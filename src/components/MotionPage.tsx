import { useLayoutEffect, useRef, useState, type ReactNode } from 'react'

/** Lazy first entry for data-heavy pages; retain their data and filters afterwards. */
export function MotionPage({ active, enabled, lazy = false, className, children }: {
  active: boolean; enabled: boolean; lazy?: boolean; className?: string; children: ReactNode
}) {
  const [visited, setVisited] = useState(active)
  const element = useRef<HTMLDivElement>(null)
  useLayoutEffect(() => { if (active) setVisited(true) }, [active])
  useLayoutEffect(() => {
    if (!active || !enabled || !element.current?.animate) return
    const animation = element.current.animate([{ opacity: .78 }, { opacity: 1 }], {
      duration: 130, easing: 'cubic-bezier(.16,1,.3,1)',
    })
    const stop = () => animation.cancel()
    const visibility = () => { if (document.hidden) stop() }
    window.addEventListener('blur', stop)
    document.addEventListener('visibilitychange', visibility)
    return () => {
      stop(); window.removeEventListener('blur', stop)
      document.removeEventListener('visibilitychange', visibility)
    }
  }, [active, enabled])
  if (lazy && !visited && !active) return null
  return <div ref={element} className={className ? `motion-page ${className}` : 'motion-page'} hidden={!active}>{children}</div>
}
