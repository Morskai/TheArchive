import { t, message as uiMessage, useLocale, type UiText } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import { addFilmCastActor, listCastCandidates, removeFilmCastActor, type FilmCastActor, type FilmCastCandidate } from '../data/realFilms'
import { castSaveMessage, filterCastCandidates } from '../data/filmCast'

export function FilmCastEditor({ filmId, actors, editing }: { filmId: string; actors: FilmCastActor[]; editing: boolean }) {
  useLocale()
  const [query, setQuery] = useState('')
  const [candidates, setCandidates] = useState<FilmCastCandidate[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState(false)
  const [retry, setRetry] = useState(0)
  const [saving, setSaving] = useState<string | null>(null)
  const [saveError, setSaveError] = useState('')
  const [notice, setNotice] = useState<UiText>('')
  const inputRef = useRef<HTMLInputElement>(null)
  const alive = useRef(false)
  const guard = useRef(false)
  useEffect(() => { alive.current = true; return () => { alive.current = false } }, [])
  useEffect(() => {
    if (!editing) return
    let active = true
    setQuery(''); setCandidates([]); setLoading(true); setLoadError(false)
    setSaveError(''); setNotice('')
    inputRef.current?.focus({ preventScroll: true })
    void listCastCandidates(filmId).then(rows => { if (active) setCandidates(rows) })
      .catch(() => { if (active) setLoadError(true) })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [filmId, editing, retry])
  const save = async (actor: { id: string; name: string }, remove: boolean) => {
    if (guard.current) return
    guard.current = true; setSaving(actor.id); setSaveError(''); setNotice('')
    try {
      await (remove ? removeFilmCastActor(filmId, actor.id) : addFilmCastActor(filmId, actor.id))
      if (alive.current) setNotice(uiMessage(remove ? "已移除出演者：{0}。" : "已添加出演者：{0}。", [actor.name]))
    } catch (error) { if (alive.current) setSaveError(castSaveMessage(error)) }
    finally { guard.current = false; if (alive.current) setSaving(null) }
  }
  if (!editing) return null
  const matches = filterCastCandidates(candidates, actors, query)
  return <div className="film-cast-editor" id="film-cast-editor" aria-busy={saving !== null}>
    {actors.length > 0 && <ul className="film-cast-current" aria-label={t("当前出演表")}>{actors.map(actor => <li key={actor.id}><span className="film-cast-name">{actor.name}</span><span className="film-cast-origin">{actor.system ? t("目录识别") : t("人工指定")}</span><button type="button" disabled={saving !== null} onClick={() => void save(actor, true)} aria-label={t("从出演表移除{0}", [actor.name])}>{saving === actor.id ? t("保存中…") : t("移除")}</button></li>)}</ul>}
    <label className="film-cast-search"><span>{t("添加演员")}</span><input ref={inputRef} type="search" value={query} onChange={event => setQuery(event.target.value)} placeholder={t("搜索已保存的演员")} aria-label={t("搜索已保存的演员")} autoComplete="off" /></label>
    <p className="film-cast-hint">{t("从已启用片库选择，添加或移除后立即保存。")}</p>
    {saveError && <p className="real-save-error" role="alert">{t(saveError)}</p>}
    {notice && <p className="film-cast-notice" role="status">{t(notice)}</p>}
    {loading ? <p className="real-detail-empty" role="status">{t("正在读取可选演员…")}</p> : loadError ? <div className="film-cast-load-error"><p className="real-save-error" role="alert">{t("演员列表暂时无法读取，请重试。")}</p><button type="button" onClick={() => setRetry(value => value + 1)}>{t("重试")}</button></div> : matches.length ? <ul className="film-cast-candidates" aria-label={t("可添加的演员")}>{matches.map(actor => <li key={actor.id}><div><span className="film-cast-name">{actor.name}</span><small>{actor.library}</small></div><button type="button" disabled={saving !== null} onClick={() => void save(actor, false)} aria-label={t("添加{0}到出演表", [actor.name])}>{saving === actor.id ? t("保存中…") : t("添加")}</button></li>)}</ul> : <p className="real-detail-empty" role="status">{query.trim() ? t("没有匹配的演员，请尝试其他姓名。") : candidates.length ? t("当前可选演员均已在出演表中。") : t("已启用片库中暂无可添加的演员。")}</p>}
  </div>
}
