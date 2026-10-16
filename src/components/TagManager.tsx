import { t, useLocale } from '../i18n/react'
import { useEffect, useRef, useState, type FormEvent } from 'react'

interface TagManagerProps {
  tags: string[]
  onAdd: (tag: string) => void | Promise<void>
  onDelete: (tag: string) => void | Promise<void>
  onRename: (tag: string, name: string) => void | Promise<void>
  onClose: () => void
  subject?: '影片' | '演员'
}

export function TagManager({ tags, onAdd, onDelete, onRename, onClose, subject = '影片' }: TagManagerProps) {
  useLocale()
  const [name, setName] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [editing, setEditing] = useState(false)
  const [drafts, setDrafts] = useState<Record<string, string>>({})
  const dialogRef = useRef<HTMLElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    inputRef.current?.focus()
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { onClose(); return }
      if (event.key !== 'Tab' || !dialogRef.current) return
      const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>('input, button:not(:disabled)')]
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => { window.removeEventListener('keydown', onKeyDown); previousFocus?.focus() }
  }, [onClose])

  const addTag = async (event: FormEvent) => {
    event.preventDefault()
    const trimmed = name.trim()
    if (!trimmed) { setError('请输入标签名称'); return }
    if (tags.some(tag => tag.toLocaleLowerCase() === trimmed.toLocaleLowerCase())) { setError('这个标签已经存在'); return }
    if (tags.length >= 30) { setError('最多可创建30个标签，请先删除不需要的标签。'); return }
    setBusy(true)
    try {
      await onAdd(trimmed)
      setName('')
      setError('')
      inputRef.current?.focus()
    } catch { setError('标签未保存，请重试。') }
    finally { setBusy(false) }
  }

  const removeTag = async (tag: string) => {
    setBusy(true)
    try { await onDelete(tag); setError('') }
    catch { setError('标签未删除，请重试。') }
    finally { setBusy(false) }
  }

  const renameTag = async (tag: string) => {
    if (busy) return false
    const value = (Object.hasOwn(drafts, tag) ? drafts[tag] : tag).trim()
    if (value === tag) return true
    if (!value || [...value].length > 20) { setError('标签名称应为1至20个字符'); return false }
    if (tags.some(other => other !== tag && other.toLocaleLowerCase() === value.toLocaleLowerCase())) { setError('这个标签已经存在'); return false }
    setBusy(true)
    try { await onRename(tag, value); setDrafts(current => { const next = { ...current }; delete next[tag]; return next }); setError(''); return true }
    catch (reason) { setError(String(reason).includes('duplicate_tag') ? '这个标签已经存在' : '标签未保存，请重试。'); return false }
    finally { setBusy(false) }
  }

  const toggleEditing = async () => {
    if (editing) for (const tag of tags) if (Object.hasOwn(drafts, tag) && !await renameTag(tag)) return
    setEditing(value => !value); setDrafts({}); setError('')
  }

  return <div className="tag-manager-layer">
    <button className="tag-manager-backdrop" type="button" aria-label={t("关闭标签编辑")} onClick={onClose} />
    <section className="tag-manager-dialog" ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="tag-manager-title">
      <div className="tag-manager-head"><div><span className="section-kicker">{t("收藏／标签")}</span><h2 id="tag-manager-title">{t("标签编辑")}</h2></div><button className="close-button dialog-close-button" type="button" onClick={onClose} aria-label={t("关闭标签编辑")}>×</button></div>
      <form className="tag-create" onSubmit={addTag}><label htmlFor="new-tag">{t("新建标签")}</label><div><input id="new-tag" ref={inputRef} value={name} onChange={event => { setName(event.target.value); setError('') }} maxLength={20} placeholder={t("输入标签名称")} disabled={busy} /><button type="submit" disabled={busy}>{t("添加标签")}</button></div>{error && <p className="tag-error" role="alert">{t(error)}</p>}</form>
      <div className="tag-manager-list-head">{t("全部标签")}<span>{tags.length} / 30</span><button type="button" disabled={busy} aria-pressed={editing} onPointerDown={event => event.preventDefault()} onClick={() => void toggleEditing()}>{editing ? t("完成编辑") : t("编辑")}</button></div>
      {tags.length === 0 ? <p className="tag-manager-empty">{t("暂无标签，先创建一个。")}</p> : <ul className="tag-manager-list">{tags.map(tag => <li key={tag}>{editing ? <input aria-label={t("编辑标签 {0}", [tag])} value={Object.hasOwn(drafts, tag) ? drafts[tag] : tag} maxLength={20} disabled={busy} onChange={event => setDrafts(current => ({ ...current, [tag]: event.target.value }))} onBlur={() => void renameTag(tag)} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); event.currentTarget.blur() } }} /> : <span title={tag}>{tag}</span>}<button type="button" disabled={busy} aria-label={t("删除标签 {0}", [tag])} onClick={() => removeTag(tag)}>{t("删除")}</button></li>)}</ul>}
      <p className="tag-manager-help">{t("删除标签会同时从已标记的{0}中移除。", [t(subject)])}</p>
    </section>
  </div>
}
