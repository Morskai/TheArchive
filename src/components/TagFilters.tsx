import { t, useLocale } from '../i18n/react'
import { useEffect, useId, useLayoutEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import { moveTag, placeTag } from '../data/tagPresentation'

type Drag = { pointerId: number; tag: string; base: string[]; order: string[]; x: number; y: number; originX: number; originY: number; width: number; height: number; active: boolean }
type Props = { tags: string[]; counts: Record<string, number>; showCounts: boolean; selected: string[]; onToggle: (tag: string) => void; onReorder: (tags: string[]) => void | Promise<void> }
const sameOrder = (a: string[], b: string[]) => a.length === b.length && a.every((tag, index) => tag === b[index])

export function TagFilters({ tags, counts, showCounts, selected, onToggle, onReorder }: Props) {
  useLocale()
  const [expanded, setExpanded] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [draft, setDraft] = useState<string[] | null>(null)
  const [ghost, setGhost] = useState<Drag | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const drag = useRef<Drag | null>(null)
  const pending = useRef(false)
  const props = useRef({ tags, onReorder })
  props.current = { tags, onReorder }
  const positions = useRef(new Map<string, { x: number; y: number }>())
  const animations = useRef<Animation[]>([])
  const id = useId()
  const order = draft ?? tags

  const cancel = () => {
    const current = drag.current
    drag.current = null
    if (current && root.current?.hasPointerCapture(current.pointerId)) root.current.releasePointerCapture(current.pointerId)
    setGhost(null); setDraft(null)
  }
  const save = async (next: string[]) => {
    if (pending.current || sameOrder(next, props.current.tags)) { setDraft(null); return }
    pending.current = true; setBusy(true); setError(''); setDraft(next)
    try { await props.current.onReorder(next) }
    catch { setError('排序未保存，请重试。') }
    finally { pending.current = false; setBusy(false); setDraft(null) }
  }
  const saveRef = useRef(save)
  saveRef.current = save

  useEffect(() => {
    // A refresh/deletion must not let a gesture submit an obsolete tag inventory.
    if (drag.current && !sameOrder(drag.current.base, tags)) cancel()
  }, [tags])

  useEffect(() => {
    const move = (event: PointerEvent) => {
      const current = drag.current
      if (!current || event.pointerId !== current.pointerId) return
      if (!current.active && Math.hypot(event.clientX - current.originX, event.clientY - current.originY) < 5) return
      current.active = true; current.x = event.clientX; current.y = event.clientY
      event.preventDefault()
      const hit = document.elementFromPoint(event.clientX, event.clientY)
      if (hit && root.current?.contains(hit)) {
        const item = hit.closest<HTMLElement>('[data-tag-sort-item]')
        const preview = hit.closest<HTMLElement>('.tag-filter-preview')
        const list = hit.closest<HTMLElement>('.tag-filter-all')
        if (list) {
          const bounds = list.getBoundingClientRect()
          if (event.clientY < bounds.top + 28) list.scrollTop -= 16
          else if (event.clientY > bounds.bottom - 28) list.scrollTop += 16
        }
        if (item?.dataset.tag && item.dataset.tag !== current.tag) {
          const bounds = item.getBoundingClientRect()
          current.order = placeTag(current.order, current.tag, item.dataset.tag, event.clientX < bounds.left + bounds.width / 2)
        } else if (preview && !item) {
          // Empty row space is a valid destination, including when all chips are clipped.
          const bounds = preview.getBoundingClientRect()
          const visible = Array.from(preview.querySelectorAll<HTMLElement>('[data-tag-sort-item]'))
            .filter(element => element.getBoundingClientRect().left < bounds.right && element.getBoundingClientRect().right > bounds.left)
          const target = event.clientX < bounds.left + 16 ? visible[0] : visible.at(-1)
          if (target?.dataset.tag) current.order = placeTag(current.order, current.tag, target.dataset.tag, event.clientX < bounds.left + 16)
        }
      }
      setDraft(current.order); setGhost({ ...current })
    }
    const finish = (event: PointerEvent) => {
      const current = drag.current
      if (!current || event.pointerId !== current.pointerId) return
      drag.current = null
      if (root.current?.hasPointerCapture(current.pointerId)) root.current.releasePointerCapture(current.pointerId)
      setGhost(null)
      const hit = document.elementFromPoint(event.clientX, event.clientY)
      const destination = hit?.closest('.tag-filter-preview,.tag-filter-all')
      if (current.active && destination && root.current?.contains(destination) && sameOrder(current.base, props.current.tags)) void saveRef.current(current.order)
      else setDraft(null)
    }
    const abort = () => cancel()
    const pointerCancel = (event: PointerEvent) => { if (drag.current?.pointerId === event.pointerId) abort() }
    const visibility = () => { if (document.hidden) abort() }
    const key = (event: KeyboardEvent) => { if (event.key === 'Escape' && drag.current) { event.preventDefault(); abort() } }
    const lostCapture = () => { if (drag.current) abort() }
    const element = root.current
    window.addEventListener('pointermove', move, { passive: false })
    window.addEventListener('pointerup', finish)
    window.addEventListener('pointercancel', pointerCancel)
    window.addEventListener('blur', abort)
    window.addEventListener('resize', abort)
    document.addEventListener('visibilitychange', visibility)
    window.addEventListener('keydown', key)
    element?.addEventListener('lostpointercapture', lostCapture)
    return () => {
      const current = drag.current
      drag.current = null
      if (current && element?.hasPointerCapture(current.pointerId)) element.releasePointerCapture(current.pointerId)
      window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', finish)
      window.removeEventListener('pointercancel', pointerCancel); window.removeEventListener('blur', abort); window.removeEventListener('keydown', key)
      window.removeEventListener('resize', abort); document.removeEventListener('visibilitychange', visibility)
      element?.removeEventListener('lostpointercapture', lostCapture)
      animations.current.forEach(animation => animation.cancel())
    }
  }, [])

  useLayoutEffect(() => {
    const element = root.current
    if (!element) return
    animations.current.forEach(animation => animation.cancel())
    animations.current = []
    const next = new Map<string, { x: number; y: number }>()
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches || element.closest('[data-motion="false"]') !== null
    element.querySelectorAll<HTMLElement>('[data-tag-sort-item]').forEach(item => {
      const key = item.dataset.tagSortItem!
      const bounds = item.getBoundingClientRect()
      const surface = item.closest<HTMLElement>('.tag-filter-preview,.tag-filter-all')!
      const surfaceBounds = surface.getBoundingClientRect()
      // Measure layout within the scrolling surface so scrolling does not become a slide.
      const x = bounds.left - surfaceBounds.left + surface.scrollLeft
      const y = bounds.top - surfaceBounds.top + surface.scrollTop
      const previous = positions.current.get(key)
      next.set(key, { x, y })
      if (!reduce && previous && (previous.x !== x || previous.y !== y)) {
        animations.current.push(item.animate([
          { transform: `translate(${previous.x - x}px, ${previous.y - y}px)` },
          { transform: 'translate(0, 0)' },
        ], { duration: 190, easing: 'cubic-bezier(.2,.8,.2,1)' }))
      }
    })
    positions.current = next
  }, [order, expanded])

  const start = (event: ReactPointerEvent<HTMLButtonElement>, tag: string) => {
    if (pending.current || event.button !== 0 || !event.isPrimary || drag.current) return
    event.preventDefault(); event.stopPropagation(); event.currentTarget.focus({ preventScroll: true })
    const bounds = event.currentTarget.parentElement!.getBoundingClientRect()
    drag.current = { pointerId: event.pointerId, tag, base: [...tags], order: [...tags], x: event.clientX, y: event.clientY, originX: event.clientX, originY: event.clientY, width: bounds.width, height: bounds.height, active: false }
    root.current?.setPointerCapture(event.pointerId)
    setError('')
  }
  const item = (tag: string, surface: 'preview' | 'list') => <div
    className={`tag-filter-item ${surface === 'preview' ? 'tag-preview-item' : ''} ${selected.includes(tag) ? 'selected' : ''} ${ghost?.tag === tag ? 'dragging' : ''}`}
    data-tag-sort-item={`${surface}:${tag}`} data-tag={tag} key={tag}>
    <button type="button" className="tag-filter-choice" aria-pressed={selected.includes(tag)} title={tag} onClick={() => { if (!drag.current?.active) onToggle(tag) }}><span>{tag}</span>{showCounts && <small className="tag-film-count">{counts[tag] ?? 0}</small>}</button>
    <button type="button" className="tag-sort-handle" title={t("拖动排序；方向键调整顺序")} aria-label={t("调整{0}的顺序", [tag])} disabled={busy}
      onPointerDown={event => start(event, tag)} onClick={event => { event.preventDefault(); event.stopPropagation() }}
      onKeyDown={event => {
        if (['ArrowUp', 'ArrowLeft', 'ArrowDown', 'ArrowRight'].includes(event.key)) {
          event.preventDefault()
          if (drag.current || pending.current) return
          const index = tags.indexOf(tag)
          const target = tags[index + (event.key === 'ArrowUp' || event.key === 'ArrowLeft' ? -1 : 1)]
          if (target) void save(moveTag(tags, tag, target))
        }
      }}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6h8M4 10h8" /></svg></button>
  </div>

  return <div ref={root} className={`tag-filters ${ghost ? 'tag-filters-dragging' : ''}`}>
    <div className="tag-filter-preview" aria-label={t("标签筛选")}>{order.map(tag => item(tag, 'preview'))}</div>
    <button className="tag-list-toggle" type="button" aria-label={expanded ? t("收起标签") : t("展开标签")} title={expanded ? t("收起标签") : t("展开标签")} aria-expanded={expanded} aria-controls={id}
      onClick={() => { cancel(); setExpanded(value => !value) }}><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 6l4 4 4-4" /></svg></button>
    {expanded && <div className="tag-filter-panel" id={id} aria-label={t("全部标签")} aria-busy={busy}>
      <div className="tag-filter-panel-head"><span>{t("全部标签 · {0}", [tags.length])}</span><small>{t("拖动排序柄，可移到上方筛选行")}</small></div>
      {tags.length === 0 ? <p className="tag-filter-empty">{t("暂无标签，可在标签编辑中添加。")}</p> : <div className="tag-filter-all">{order.map(tag => item(tag, 'list'))}</div>}
    </div>}
    {error && <p className="tag-sort-error" role="alert">{t(error)}</p>}
    {ghost && <div className="tag-drag-ghost" aria-hidden="true" style={{ left: Math.max(8, Math.min(ghost.x + 12, window.innerWidth - Math.min(ghost.width, 200) - 12)), top: Math.max(8, Math.min(ghost.y + 10, window.innerHeight - ghost.height - 12)), maxWidth: Math.min(ghost.width, 200) }}><span>{ghost.tag}</span><svg viewBox="0 0 16 16"><path d="M4 6h8M4 10h8" /></svg></div>}
  </div>
}
