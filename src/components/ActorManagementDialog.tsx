import { t, useLocale } from '../i18n/react'
import { useEffect, useId, useRef, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import '../actor-management.css'

export function ActorManagementDialog({ title, busy, onClose, children, wide = false }: { title: string; busy: boolean; onClose: () => void; children: ReactNode; wide?: boolean }) {
  useLocale()
  const ref = useRef<HTMLElement>(null)
  const titleId = useId()
  const close = useRef(onClose)
  const working = useRef(busy)
  close.current = onClose
  working.current = busy
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const focusables = () => [...ref.current?.querySelectorAll<HTMLElement>('button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]') ?? []]
    const autoFocus = ref.current?.querySelector<HTMLElement>('[autofocus]')
    ;(autoFocus ?? focusables()[0])?.focus({ preventScroll: true })
    const keydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        if (ref.current?.querySelector('[role="combobox"][aria-expanded="true"]')) return
        event.preventDefault(); if (!working.current) close.current(); return
      }
      if (event.key !== 'Tab') return
      const nodes = focusables(), first = nodes[0], last = nodes.at(-1)
      if (!first) { event.preventDefault(); ref.current?.focus(); return }
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus() }
    }
    window.addEventListener('keydown', keydown)
    return () => { window.removeEventListener('keydown', keydown); previous?.focus({ preventScroll: true }) }
  }, [])
  return createPortal(<div className="actor-management-layer"><button className="actor-management-backdrop" type="button" tabIndex={-1} disabled={busy} aria-label={t("关闭{0}", [title])} onClick={onClose} /><section ref={ref} className={`actor-management-dialog${wide ? ' actor-management-wide' : ''}`} role="dialog" aria-modal="true" aria-labelledby={titleId} aria-busy={busy} tabIndex={-1}>
    <div className="actor-management-head"><h2 id={titleId}>{title}</h2><button className="close-button dialog-close-button" type="button" disabled={busy} onClick={onClose} aria-label={t("关闭{0}", [title])}>×</button></div>
    {children}
  </section></div>, document.body)
}
