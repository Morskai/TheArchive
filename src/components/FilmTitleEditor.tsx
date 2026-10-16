import { t, useLocale, message } from '../i18n/react'
import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from 'react'
import type { RealFilm } from '../data/realFilms'
import { editFilmTitle } from '../data/titleTranslationApi'
import { translationError } from '../data/titleTranslation'
import { notifyRealLibraryChanged } from '../data/playback'
import { FilmCodeEditor } from './FilmCodeEditor'
import { EditIcon } from './EditIcon'

export interface FilmTitleEditorHandle { finish: () => void }

export const FilmTitleEditor = forwardRef<FilmTitleEditorHandle, { film: RealFilm; code: string }>(function FilmTitleEditor({ film, code }, ref) {
  useLocale()
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(film.title)
  const [title, setTitle] = useState(film.title)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const input = useRef<HTMLTextAreaElement>(null)
  const editButton = useRef<HTMLButtonElement>(null)
  const session = useRef<{ original: string; title: string } | null>(null)
  const writing = useRef(false)
  const mounted = useRef(true)
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  useEffect(() => { if (!session.current && !writing.current) { setTitle(film.title); setDraft(film.title) } }, [film.title])
  useEffect(() => { if (editing) { input.current?.focus(); input.current?.select() } }, [editing])
  const finish = (returnFocus = false) => {
    const current = session.current
    if (!current || writing.current) return
    session.current = null
    setEditing(false)
    const value = draft.trim()
    if (value === current.title) { if (returnFocus) window.requestAnimationFrame(() => editButton.current?.focus()); return }
    writing.current = true
    setSaving(true); setError('')
    void editFilmTitle(film.id, current.original, current.title, value).then(saved => {
      if (mounted.current) { setTitle(saved.title); setDraft(saved.title) }
      notifyRealLibraryChanged()
    }).catch(cause => {
      if (mounted.current) { setTitle(current.title); setError(translationError(cause)) }
    }).finally(() => { writing.current = false; if (mounted.current) { setSaving(false); if (returnFocus) window.requestAnimationFrame(() => editButton.current?.focus()) } })
  }
  useImperativeHandle(ref, () => ({ finish }))
  return <>
    <FilmCodeEditor film={film} code={code} />
    <div className="detail-title-row">
      {!editing && <h2 id="detail-title">{title}</h2>}
      <button ref={editButton} type="button" className="detail-edit-icon" title={saving ? t("正在保存…") : t("编辑片名")} aria-label={t("编辑片名")} disabled={saving || editing} aria-busy={saving} aria-expanded={editing} onClick={() => {
        session.current = { original: film.original_title ?? '', title }
        if (!error) setDraft(title)
        setError(''); setEditing(true)
      }}><EditIcon /></button>
    </div>
    {editing ? <div className="detail-title-input-wrap"><span id="detail-title" className="visually-hidden">{t("编辑影片片名")}</span><textarea ref={input} className="detail-title-input" aria-label={t("影片片名")} rows={3} maxLength={20000} value={draft} onChange={event => setDraft(event.target.value)} onBlur={() => finish()} onKeyDown={event => {
      if (event.key === 'Enter' && !event.nativeEvent.isComposing && event.keyCode !== 229) { event.preventDefault(); finish(true) }
    }} /></div> : null}
    {error && <p className="real-save-error" role="alert">{t("{0}点击“编辑片名”可重试。", [message(error)])}</p>}
  </>
})
