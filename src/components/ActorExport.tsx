import { actorManagementMessage as actorManagementError } from '../i18n/feedback'
import { t, message as uiMessage, useLocale, type UiText } from '../i18n/react'
import { useRef, useState } from 'react'
import type { Actor } from '../data/actors'
import { chooseActorExportDestination, confirmActorExport, previewActorExport } from '../data/actorManagement'

import { ActorManagementDialog } from './ActorManagementDialog'

export function ActorExport({ actors, onClose }: { actors: Actor[]; onClose: () => void }) {
  useLocale()
  const [actorIds, setActorIds] = useState(() => actors.map(actor => actor.id))
  const [destination, setDestination] = useState('')
  const [exportedDirectory, setExportedDirectory] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<UiText>('')
  const [notice, setNotice] = useState<UiText>('')
  const guard = useRef(false)
  const run = async (action: () => Promise<void>) => {
    if (guard.current) return
    guard.current = true; setBusy(true); setError(''); setNotice('')
    try { await action() } catch (reason) { setError(actorManagementError(reason)) }
    finally { guard.current = false; setBusy(false) }
  }
  const chooseDestination = () => run(async () => {
    const chosen = await chooseActorExportDestination()
    if (chosen) setDestination(chosen)
  })
  const write = () => run(async () => {
    if (!destination || !actorIds.length) return
    const preview = await previewActorExport(actorIds, destination)
    const overwrite = preview.items.some(item => item.exists || item.previousDirectories.length > 0)
    const result = await confirmActorExport(actorIds, destination, preview.token, overwrite)
    setExportedDirectory(result.directory); setNotice(uiMessage("已导出 {0} 位演员资料。", [result.exported]))
  })
  return <ActorManagementDialog title={t("导出资料")} busy={busy} onClose={onClose} wide>
    <p className="actor-management-hint">{t("选择导出位置，每位演员一个文件夹，保存 actor.info 资料与出演清单，有照片时附 actor.jpg。重装前请存到软件资料目录之外。")}</p>
    {!notice && <div className="actor-export-destination"><button className="actor-management-action" type="button" disabled={busy} onClick={() => void chooseDestination()}>{t("选择导出位置")}</button>{destination && <p className="actor-management-path">{destination.replace(/[\\/]$/, '')} / ActorExports</p>}</div>}
    {!notice && <><div className="actor-export-select-all"><label><input type="checkbox" checked={actorIds.length === actors.length && actors.length > 0} disabled={busy} onChange={event => setActorIds(event.target.checked ? actors.map(actor => actor.id) : [])} />{t("全选演员")}</label><span>{t("已选 {0}位", [actorIds.length])}</span></div><ul className="actor-export-options">{actors.map(actor => <li key={actor.id}><label><input type="checkbox" disabled={busy} checked={actorIds.includes(actor.id)} onChange={event => setActorIds(ids => event.target.checked ? [...ids, actor.id] : ids.filter(id => id !== actor.id))} /><span>{actor.name}<small>{actor.aliases || t("无别名")} · {actor.id.slice(-8)}</small></span></label></li>)}</ul><p className="actor-export-warning">{t("确认导出会更新该位置已有的软件导出，并移除同一演员改名前的旧导出目录。")}</p></>}
    {busy && <p className="actor-management-hint" role="status">{t("正在处理演员导出…")}</p>}{error && <p className="actor-management-error" role="alert">{t(error)}</p>}{notice && <div className="actor-management-notice" role="status"><p>{t(notice)}</p><p className="actor-management-path">{exportedDirectory}</p></div>}
    <div className="actor-management-footer"><button className="actor-management-action" type="button" disabled={busy} onClick={onClose}>{notice ? t("完成") : t("取消")}</button>{!notice && <button className="actor-management-primary" type="button" disabled={busy || !actorIds.length || !destination} onClick={() => void write()}>{t("确认导出")}</button>}</div>
  </ActorManagementDialog>
}
