import { t, useLocale } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { isTauri } from '@tauri-apps/api/core'
import { invoke } from '../data/diagnostics.ts'
import { editText } from '../data/textEditing'
import './text-edit-menu.css'

type Field = HTMLInputElement | HTMLTextAreaElement
const entries = [['剪切', 'Ctrl+X'], ['复制', 'Ctrl+C'], ['粘贴', 'Ctrl+V'], ['全选', 'Ctrl+A']] as const
function textField(target: EventTarget | null): Field | null {
  if (target instanceof HTMLTextAreaElement && !target.disabled) return target
  if (target instanceof HTMLInputElement && !target.disabled && !['checkbox', 'radio', 'file', 'range', 'color', 'hidden', 'button', 'submit', 'reset'].includes(target.type)) return target
  return null
}
const writeClipboard = (text: string) => isTauri() ? invoke<void>('write_edit_clipboard', { text }) : navigator.clipboard.writeText(text)
const readClipboard = () => isTauri() ? invoke<string>('read_edit_clipboard') : navigator.clipboard.readText()

export function TextEditMenu() {
  useLocale()
  const [menu, setMenu] = useState<{ field: Field; x: number; y: number; start: number; end: number; value: string } | null>(null)
  const [error, setError] = useState('')
  const [active, setActive] = useState(0)
  const [busy, setBusy] = useState(false)
  const busyRef = useRef(false)
  const element = useRef<HTMLDivElement>(null)
  const state = useRef(menu)
  state.current = menu
  useEffect(() => {
    const open = (event: MouseEvent) => {
      event.preventDefault()
      if (element.current?.contains(event.target as Node)) return
      const field = textField(event.target)
      if (!field) { setMenu(null); return }
      field.focus({ preventScroll: true })
      setMenu({ field, value: field.value, start: field.selectionStart ?? 0, end: field.selectionEnd ?? field.value.length, x: Math.max(8, Math.min(event.clientX, window.innerWidth - 208)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - 230)) })
      setActive(0); setError('')
    }
    const close = (event: PointerEvent) => { if (!element.current?.contains(event.target as Node)) setMenu(null) }
    const dismiss = () => setMenu(null)
    document.addEventListener('contextmenu', open)
    document.addEventListener('pointerdown', close)
    window.addEventListener('blur', dismiss)
    window.addEventListener('resize', dismiss)
    document.addEventListener('scroll', dismiss, true)
    return () => { document.removeEventListener('contextmenu', open); document.removeEventListener('pointerdown', close); window.removeEventListener('blur', dismiss); window.removeEventListener('resize', dismiss); document.removeEventListener('scroll', dismiss, true) }
  }, [])
  const disabled = (index: number) => !menu || busy || (index === 0 || index === 2) && menu.field.readOnly || (index === 0 || index === 1) && menu.start === menu.end
  const execute = async (index: number) => {
    const current = state.current
    if (!current || disabled(index) || busyRef.current) return
    const { field, value, start, end } = current
    busyRef.current = true; setBusy(true); setError('')
    try {
      if (index === 3) {
        field.select()
        // Selection API is unavailable on number/date fields; the captured whole value remains usable.
        setMenu(null); return
      }
      if (index < 2) await writeClipboard(value.slice(start, end))
      if (index === 0 || index === 2) {
        const inserted = index === 0 ? '' : await readClipboard()
        if (state.current !== current || !field.isConnected || field.value !== value || document.activeElement !== field) { setMenu(null); return }
        const next = editText(value, start, end, inserted, field.maxLength)
        // Setting the prototype value bypasses React's tracker, so controlled fields receive onChange.
        const prototype = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype
        Object.getOwnPropertyDescriptor(prototype, 'value')!.set!.call(field, next.value)
        if (field.value !== next.value) { Object.getOwnPropertyDescriptor(prototype, 'value')!.set!.call(field, value); throw new Error('invalid_value') }
        field.dispatchEvent(new Event('input', { bubbles: true }))
        try { field.setSelectionRange(next.caret, next.caret) } catch { /* Number/date inputs have no selection range. */ }
      }
      setMenu(null)
    } catch { setError('操作未完成，请重试或使用快捷键。') }
    finally { busyRef.current = false; setBusy(false) }
  }
  useEffect(() => {
    if (!menu) return
    const keydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopImmediatePropagation(); setMenu(null) }
      else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); setActive(value => (value + (event.key === 'ArrowDown' ? 1 : 3)) % 4) }
      else if (event.key === 'Enter') { event.preventDefault(); event.stopImmediatePropagation(); void execute(active) }
      else if (event.key === 'Tab') setMenu(null)
    }
    document.addEventListener('keydown', keydown, true)
    return () => document.removeEventListener('keydown', keydown, true)
  })
  if (!menu) return null
  return createPortal(<div ref={element} className="text-edit-menu" role="menu" aria-label={t("文本编辑")} style={{ left: menu.x, top: menu.y }} onPointerDown={event => event.preventDefault()} onMouseDown={event => event.preventDefault()}>
    {entries.map(([label, shortcut], index) => <button key={label} role="menuitem" tabIndex={-1} type="button" disabled={disabled(index)} className={index === active ? 'active' : ''} onPointerEnter={() => setActive(index)} onClick={() => void execute(index)}><span>{t(label)}</span><kbd>{shortcut}</kbd></button>)}
    {error && <p role="alert">{t(error)}</p>}
  </div>, document.body)
}
