import { hasAmbiguousFilmCode } from '../data/realFilmPresentation'
import { t, useLocale } from '../i18n/react'
import { useState, useRef } from 'react'
import { invoke } from '../data/diagnostics.ts'
import type { RealFilm } from '../data/realFilms'
import { notifyRealLibraryChanged } from '../data/playback'
import { EditIcon } from './EditIcon'
export function FilmCodeEditor({ film, code }: { film: RealFilm; code: string }) {
  useLocale()
 const [editing,setEditing]=useState(false), [draft,setDraft]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState('')
 const guard=useRef(false)
 const save=async()=>{if(guard.current)return;guard.current=true;setBusy(true);setError('');try{await invoke('set_film_code',{filmId:film.id,value:draft.trim()});setEditing(false);notifyRealLibraryChanged()}catch{setError('影片编号未保存，请重试。')}finally{guard.current=false;setBusy(false)}}
 return <div className="film-code-editor">{editing?<form onSubmit={e=>{e.preventDefault();void save()}}><label>{t("影片编号")}<input autoFocus maxLength={200} value={draft} onChange={e=>setDraft(e.target.value)} aria-describedby="film-code-hint" /></label><p id="film-code-hint">{t("留空恢复目录识别值。")}</p><div className="settings-inline-actions"><button className="detail-title-edit" disabled={busy} type="submit">{busy?t("正在保存…"):t("保存编号")}</button><button className="detail-title-edit" type="button" disabled={busy} onClick={()=>{setEditing(false);setError('')}}>{t("取消")}</button></div></form>:<><span className={`detail-code ${hasAmbiguousFilmCode(film)?'real-code-placeholder':''}`}>{hasAmbiguousFilmCode(film) ? t(code) : code}</span><button className="detail-edit-icon" title={t("编辑编号")} aria-label={t("编辑编号")} type="button" onClick={()=>{setDraft(film.manual_code??'');setEditing(true);setError('')}}><EditIcon /></button></>}{error&&<p role="alert" className="real-save-error">{t(error)}</p>}</div>
}
