import { Children, isValidElement, useEffect, useId, useRef, useState, type ReactNode, type SelectHTMLAttributes } from 'react'
import { createPortal } from 'react-dom'
import './app-select.css'

type Choice = { value: string; label: ReactNode; disabled: boolean }
type Props = Omit<SelectHTMLAttributes<HTMLSelectElement>, 'onChange' | 'children' | 'multiple' | 'size' | 'defaultValue'> & {
  children: ReactNode
  onChange?: (event: { target: { value: string } }) => void
  popupSizing?: 'content'
  compact?: boolean
}

// One keyboard and popup contract for every controlled dropdown in the application.
export function AppSelect({ children, value, disabled, onChange, popupSizing, compact = false, className = '', style, id, ...props }: Props) {
  const choices: Choice[] = Children.toArray(children).flatMap(child => {
    if (!isValidElement<{ value?: string | number; children?: ReactNode; disabled?: boolean }>(child) || child.type !== 'option') return []
    return [{ value: String(child.props.value ?? ''), label: child.props.children, disabled: Boolean(child.props.disabled) }]
  })
  const selected = choices.findIndex(choice => choice.value === String(value))
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(0)
  const [position, setPosition] = useState({ left: 0, top: 0, width: 0, height: 240 })
  const root = useRef<HTMLSpanElement>(null), trigger = useRef<HTMLButtonElement>(null), menu = useRef<HTMLDivElement>(null)
  const listId = useId()
  const search = useRef({ text: '', at: 0 })
  const enabled = choices.map((choice, index) => choice.disabled ? -1 : index).filter(index => index >= 0)
  const show = (index = selected >= 0 && !choices[selected].disabled ? selected : enabled[0]) => {
    if (disabled || !enabled.length || !root.current) return
    const rect = root.current.getBoundingClientRect(), height = Math.min(264, choices.length * 36 + 10) * (compact ? 2 / 3 : 1)
    const below = window.innerHeight - rect.bottom - 10, above = rect.top - 10
    const fitsBelow = below >= height || below >= above
    const actualHeight = Math.max(40, Math.min(height, fitsBelow ? below : above))
    const textMeasure = document.createElement('canvas').getContext('2d')
    if (textMeasure && trigger.current) textMeasure.font = `12px ${getComputedStyle(trigger.current).fontFamily}`
    const labelWidth = Math.max(0, ...choices.map(choice => {
      const text = Children.toArray(choice.label).filter(part => typeof part === 'string' || typeof part === 'number').join('')
      return textMeasure?.measureText(text).width ?? text.length * 12
    }))
    const width = popupSizing === 'content'
      ? Math.min(window.innerWidth - 16, 360, Math.max(rect.width, Math.ceil(labelWidth) + 72))
      : Math.min(window.innerWidth - 16, Math.max(rect.width, 164, Math.ceil(labelWidth) + 60))
    setPosition({ left: Math.max(8, Math.min(rect.left, window.innerWidth - width - 8)), top: fitsBelow ? rect.bottom + 5 : rect.top - actualHeight - 5, width, height: actualHeight })
    setActive(index ?? 0); setOpen(true)
  }
  const choose = (index: number) => {
    if (!choices[index] || choices[index].disabled) return
    onChange?.({ target: { value: choices[index].value } }); setOpen(false); trigger.current?.focus({ preventScroll: true })
  }
  useEffect(() => {
    if (!open) return
    const outside = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node) && !menu.current?.contains(event.target as Node)) setOpen(false) }
    const scroll = (event: Event) => { if (!menu.current?.contains(event.target as Node)) setOpen(false) }
    const close = () => setOpen(false)
    document.addEventListener('pointerdown', outside); document.addEventListener('scroll', scroll, true); window.addEventListener('resize', close); window.addEventListener('blur', close)
    return () => { document.removeEventListener('pointerdown', outside); document.removeEventListener('scroll', scroll, true); window.removeEventListener('resize', close); window.removeEventListener('blur', close) }
  }, [open])
  useEffect(() => { if (disabled) setOpen(false) }, [disabled])
  useEffect(() => { if (open) document.getElementById(`${listId}-${active}`)?.scrollIntoView({ block: 'nearest' }) }, [open, active, listId])
  return <span className={`app-select ${className}`} style={style} ref={root} data-disabled={disabled || undefined}>
    <button {...props as React.ButtonHTMLAttributes<HTMLButtonElement>} id={id} ref={trigger} className="app-select-trigger" type="button" disabled={disabled} role="combobox" aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? listId : undefined} aria-activedescendant={open ? `${listId}-${active}` : undefined}
      onClick={() => open ? setOpen(false) : show()} onBlur={event => { if (!menu.current?.contains(event.relatedTarget)) setOpen(false) }}
      onKeyDown={event => {
        if (event.key === 'Escape' && open) { event.preventDefault(); event.stopPropagation(); setOpen(false); return }
        if (event.key === 'Tab') { setOpen(false); return }
        if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
          event.preventDefault()
          const from = enabled.indexOf(active), next = event.key === 'Home' ? enabled[0] : event.key === 'End' ? enabled.at(-1) : enabled[(from + (event.key === 'ArrowDown' ? 1 : -1) + enabled.length) % enabled.length]
          if (!open) show(event.key === 'Home' || event.key === 'End' ? next : undefined); else if (next !== undefined) setActive(next)
        } else if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); if (open) choose(active); else show() }
        else if (event.key.length === 1 && !event.ctrlKey && !event.altKey && !event.metaKey) {
          const now = Date.now(); search.current = { text: (now - search.current.at < 700 ? search.current.text : '') + event.key.toLocaleLowerCase(), at: now }
          const index = enabled.find(index => String(choices[index].label).toLocaleLowerCase().startsWith(search.current.text))
          if (index !== undefined) { event.preventDefault(); if (open) setActive(index); else choose(index) }
        }
      }}><span className="app-select-value">{choices[selected]?.label ?? choices[0]?.label ?? ''}</span><svg className="app-select-chevron" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true"><path d="m4 6 4 4 4-4" /></svg></button>
    {open && createPortal(<div id={listId} ref={menu} role="listbox" className={`app-select-menu${compact ? ' is-compact' : ''}`} aria-label={props['aria-label']} style={{ left: position.left, top: position.top, width: position.width, maxHeight: position.height }}>
      {choices.map((choice, index) => <div id={`${listId}-${index}`} key={`${choice.value}-${index}`} role="option" aria-selected={index === selected} aria-disabled={choice.disabled || undefined} className={`app-select-option ${index === active ? 'is-active' : ''}`} onPointerMove={() => { if (!choice.disabled) setActive(index) }} onMouseDown={event => event.preventDefault()} onClick={() => choose(index)}><span>{choice.label}</span>{index === selected && <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="m3 8 3 3 7-7" /></svg>}</div>)}
    </div>, document.body)}
  </span>
}
