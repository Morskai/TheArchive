import { actorManagementMessage as actorManagementError } from '../i18n/feedback'
import { t, useLocale, type UiText } from '../i18n/react'
import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import type { Actor } from '../data/actors'
import type { Film } from '../data/films'
import { actorAliases } from '../data/deliveryPresentation'
import { actorEditorError, actorInput, emptyActorProfile, actorEditorSources, actorEditorFilms, clearActorPersonalInfo, type ActorDraft, type ManagedActorInput } from '../data/actorManagementState'
import { chooseActorPhoto } from '../data/actorManagement'
import { realFilmsAvailable } from '../data/realFilms'
import { ActorManagementDialog } from './ActorManagementDialog'
import { RealActorPortrait } from './RealActorPortrait'
import { AppSelect } from './AppSelect'

export function ActorEditor({ actor, films, onSave, onClose }: { actor?: Actor; films: Film[]; onSave: (input: ManagedActorInput) => Promise<void>; onClose: () => void }) {
  useLocale()
  const [draft, setDraft] = useState<ActorDraft>(() => ({ name: actor?.name ?? '', aliases: actor ? actor.aliases ?? actorAliases(actor.directoryName) : '', ...(actor ? { birthMonth: actor.birthMonth, height: actor.height, cup: actor.cup, bust: actor.bust, waist: actor.waist, hips: actor.hips } : emptyActorProfile) }))
  const [filmIds, setFilmIds] = useState<string[]>(() => actor?.filmIds ?? films.filter(film => actor && film.actorIds?.includes(actor.id)).map(film => film.id))
  const [photoPath, setPhotoPath] = useState<string | null>(null)
  const [removePhoto, setRemovePhoto] = useState(false)
  const [query, setQuery] = useState('')
  const [source, setSource] = useState('all')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<UiText>('')
  const guard = useRef(false)
  const alive = useRef(true)
  useEffect(() => { alive.current = true; return () => { alive.current = false } }, [])
  const now = new Date()
  const currentMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`
  const setField = (field: keyof ActorDraft, value: string) => { setDraft(current => ({ ...current, [field]: value })); setError('') }
  const choosePhoto = async () => {
    if (guard.current) return
    guard.current = true; setBusy(true); setError('')
    try { const path = await chooseActorPhoto(); if (alive.current && path) { setPhotoPath(path); setRemovePhoto(false) } }
    catch (reason) { if (alive.current) setError(actorManagementError(reason)) }
    finally { guard.current = false; if (alive.current) setBusy(false) }
  }
  const save = async (event: FormEvent) => {
    event.preventDefault()
    if (guard.current) return
    const invalid = actorEditorError(draft, currentMonth)
    if (invalid) { setError(invalid); return }
    guard.current = true; setBusy(true); setError('')
    try { await onSave(actorInput(draft, filmIds, actor?.realActorId ?? actor?.id, photoPath, removePhoto)) }
    catch (reason) { if (alive.current) setError(actorManagementError(reason)) }
    finally { guard.current = false; if (alive.current) setBusy(false) }
  }
  const sources = useMemo(() => actorEditorSources(films), [films])
  const visibleFilms = useMemo(() => actorEditorFilms(films, query, source), [films, query, source])
  const hiddenLinks = filmIds.filter(id => !films.some(film => film.id === id)).length
  return <ActorManagementDialog title={actor ? t("编辑演员资料") : t("新建演员")} busy={busy} onClose={onClose} wide>
    <form className="actor-editor-form" onSubmit={save} noValidate>
      <div className="actor-editor-basics">
        <div className="actor-editor-photo">
          {actor?.realActorId && !removePhoto && !photoPath ? <RealActorPortrait actor={actor} large /> : <div className="actor-editor-photo-placeholder">{photoPath ? <><strong>{t("已选择照片")}</strong><span>{photoPath.split(/[\\/]/).at(-1)}</span><small>{t("保存时生成软件照片")}</small></> : <span>{removePhoto ? t("保存后移除照片") : t("照片可留空")}</span>}</div>}
          <div className="actor-photo-actions"><button className="actor-management-action" type="button" disabled={busy || !realFilmsAvailable} onClick={() => void choosePhoto()}>{actor?.hasPortrait || photoPath ? t("更换照片") : t("选择照片")}</button>{(photoPath || actor?.hasPortrait) && <button className="actor-management-action" type="button" disabled={busy || removePhoto} onClick={() => { setPhotoPath(null); setRemovePhoto(true) }}>{t("移除照片")}</button>}</div>
        </div>
        <div className="actor-profile-fields actor-editor-fields">
          <label className="actor-editor-full">{t("姓名")}<input autoFocus type="text" value={draft.name} maxLength={200} disabled={busy} onChange={event => setField('name', event.target.value)} autoComplete="off" required /></label>
          <label className="actor-editor-full">{t("别名")}<input type="text" value={draft.aliases} maxLength={1000} disabled={busy} onChange={event => setField('aliases', event.target.value)} placeholder={t("多个别名用顿号或逗号分隔")} /></label>
          <label>{t("出生年月")}<input type="text" placeholder={t("YYYY 或 YYYY-MM")} maxLength={7} autoComplete="off" disabled={busy} value={draft.birthMonth} onChange={event => setField('birthMonth', event.target.value)} /></label>
          <label>{t("身高（cm）")}<input type="number" inputMode="numeric" min="100" max="230" step="1" disabled={busy} value={draft.height} onChange={event => setField('height', event.target.value)} placeholder="100–230" /></label>
          <div className="actor-body-fields"><label>{t("罩杯")}<input type="text" maxLength={1} disabled={busy} value={draft.cup} onChange={event => setField('cup', event.target.value)} placeholder="A–Z" /></label><fieldset className="actor-measurements" aria-label={t("三围")}><div>{([['bust', t("胸围")], ['waist', t("腰围")], ['hips', t("臀围")]] as const).map(([field, label]) => <label key={field}>{t(label)}<input type="number" inputMode="numeric" min="40" max="160" step="1" disabled={busy} value={draft[field]} onChange={event => setField(field, event.target.value)} placeholder="40–160" /></label>)}</div></fieldset></div>
          <button className="actor-clear-draft" type="button" disabled={busy} onClick={() => { setDraft(clearActorPersonalInfo); setError('') }}>{t("清空个人信息")}</button>
        </div>
      </div>
      <fieldset className="actor-editor-films"><legend>{t("参演影片")}</legend><div className="actor-film-filters"><label className="actor-film-search">{t("搜索影片")}<input type="search" value={query} disabled={busy} onChange={event => setQuery(event.target.value)} placeholder={t("片名或编号")} /></label><label className="select-wrap"><span>{t("片库")}</span><AppSelect popupSizing="content" value={source} disabled={busy} onChange={event => setSource(event.target.value)} aria-label={t("参演影片片库")}><option value="all">{t("全部片库")}</option>{sources.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</AppSelect></label></div><p className="actor-selection-summary">{t("已选 {0}部", [filmIds.length])}{hiddenLinks > 0 && t(" · 当前片库未显示的 {0} 部关联保留", [hiddenLinks])}</p>
        {visibleFilms.length ? <ul className="actor-film-options">{visibleFilms.map(film => <li key={film.id}><label><input type="checkbox" checked={filmIds.includes(film.id)} disabled={busy} onChange={event => setFilmIds(ids => event.target.checked ? [...ids, film.id] : ids.filter(id => id !== film.id))} /><span>{film.title}<small>{film.code || t("暂无编号")}{film.library && ` · ${film.library}`}</small></span></label></li>)}</ul> : <p className="actor-management-empty">{query.trim() || source !== 'all' ? t("没有匹配的影片，请更换关键词或片库。") : t("暂无已保存影片，可先保存演员，稍后关联。")}</p>}
      </fieldset>
      {error && <p className="actor-management-error" role="alert">{t(error)}</p>}
      <div className="actor-management-footer"><button className="actor-management-action" type="button" disabled={busy} onClick={onClose}>{t("取消")}</button><button className="actor-management-primary" type="submit" disabled={busy}>{busy ? t("正在处理…") : actor ? t("完成编辑") : t("创建演员")}</button></div>
    </form>
  </ActorManagementDialog>
}
