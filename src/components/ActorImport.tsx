import { actorManagementMessage as actorManagementError } from '../i18n/feedback'
import { t, message as uiMessage, useLocale, type UiText } from '../i18n/react'
import { useRef, useState } from 'react'
import type { Actor } from '../data/actors'
import { chooseActorImportRoot, confirmActorScanAdd, previewActorImport } from '../data/actorManagement'
import { pendingActorScanChoice, actorScanBatchConflicts, chooseActorScanBatch, actorScanRelationships, type ActorRelationships, type ActorImportPreview, type ActorScanResolution } from '../data/actorManagementState'
import { ActorManagementDialog } from './ActorManagementDialog'
import { RealActorPortrait } from './RealActorPortrait'

export function ActorImport({ actors, onClose }: { actors: Actor[]; onClose: () => void }) {
  useLocale()
  const [preview, setPreview] = useState<ActorImportPreview | null>(null)
  const [resolutions, setResolutions] = useState<ActorScanResolution[]>([])
  const [resolving, setResolving] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<UiText>('')
  const [notice, setNotice] = useState<UiText>('')
  const [completedRelationships, setCompletedRelationships] = useState<ActorRelationships | null>(null)
  const guard = useRef(false)
  const run = async (action: () => Promise<void>) => {
    if (guard.current) return
    guard.current = true; setBusy(true); setError('')
    try { await action() } catch (reason) { setError(actorManagementError(reason)) }
    finally { guard.current = false; setBusy(false) }
  }
  const inspect = () => run(async () => {
    const chosen = await chooseActorImportRoot()
    if (!chosen) return
    setPreview(null); setNotice(''); setCompletedRelationships(null); setResolutions([]); setResolving(false)
    setPreview(await previewActorImport(chosen))
  })
  const items = preview?.items ?? []
  const current = pendingActorScanChoice(items, resolutions)
  const candidates = current ? actors.filter(actor => current.candidateIds.includes(actor.id)) : []
  const batch = !current ? actorScanBatchConflicts(items, resolutions)[0] : undefined
  const found = items.filter(item => item.status !== 'failed').length
  const failures = items.filter(item => item.status === 'failed')
  const relationships = completedRelationships ?? (preview && !current && !batch ? actorScanRelationships(items, resolutions) : null)
  const apply = (choices = resolutions) => run(async () => {
    if (!preview || !found) return
    const result = await confirmActorScanAdd(preview, choices)
    setNotice(uiMessage("已新增 {0} 位演员，更新 {1} 位，跳过 {2} 位。{3}", [result.imported, result.updated, result.skipped, result.failed ? uiMessage("另有 {0} 个目录无法导入。", [result.failed]) : '']))
    setCompletedRelationships(result.relationships)
    setPreview(null); setResolving(false)
  })
  const advance = (choices: ActorScanResolution[]) => {
    setResolutions(choices)
    if (pendingActorScanChoice(items, choices) || actorScanBatchConflicts(items, choices).length) setResolving(true)
    else setResolving(false)
  }
  const begin = () => {
    if (current || batch) advance(resolutions)
    else void apply()
  }
  const resolve = (actorId: string | null, createNew: boolean) => {
    if (!current || (createNew && !current.canCreate)) return
    const decision = { key: current.key, actorId, createNew }
    advance([...resolutions.filter(choice => choice.key !== current.key), decision])
  }
  const chooseBatch = (selectedKey: string | null) => {
    if (batch) advance(chooseActorScanBatch(items, resolutions, batch.map(item => item.key), selectedKey))
  }
  return <ActorManagementDialog title={t("扫描添加")} busy={busy} onClose={onClose}>
    <div className="actor-import-guide"><p>{t("选择存放演员资料的文件夹，扫描读取里面的actor.info文件")}</p><pre aria-label={t("演员目录结构示例")}>{t("演员资料\n└─ 演员甲\n   ├─ actor.info  演员资料（必需）\n   └─ actor.jpg   演员照片（可选）")}</pre><p className="actor-management-hint">{t("照片可留空；支持 JPG、JPEG、PNG、WebP、AVIF。")}</p></div>
    {!notice && <div className="actor-import-controls"><button className="actor-management-action" type="button" disabled={busy || resolving} onClick={() => void inspect()}>{t("选择目录")}</button></div>}
    {preview && !resolving && <div className="actor-scan-result" role="status">{t("扫描到 {0}位演员", [found])}</div>}
    {preview && !found && !failures.length && <p className="actor-management-hint" role="status">{t("没有找到 actor.info 资料文件。请检查文件夹，或改用“手动添加”。")}</p>}
    {failures.length > 0 && !resolving && <p className="actor-management-error" role="alert">{t("{0}个目录无法导入。{1}", [failures.length, actorManagementError(failures[0].reason)])}</p>}
    {resolving && current && <div className="actor-scan-conflict"><h3>{t("「{0}」是不是下面的演员？", [current.name])}</h3>{current.aliases.length > 0 && <p className="actor-management-hint">{t("这份资料中的别名：{0}", [current.aliases.join('、')])}</p>}<p className="actor-management-hint">{t("如果是同一人，请点击“更新信息”；如果非同一人，请点击“新增演员”。")}</p><div className="actor-scan-candidates">{candidates.map(actor => <div className="actor-scan-candidate" key={actor.id}>{actor.realActorId ? <RealActorPortrait actor={actor} large={false} /> : <div className="actor-scan-no-photo">{t("暂无照片")}</div>}<div><strong>{actor.name}</strong><p>{actor.aliases || t("无别名")}</p><button className="actor-management-action" type="button" disabled={busy} onClick={() => resolve(actor.id, false)}>{t("更新信息")}</button></div></div>)}</div>{!current.canCreate && <p className="actor-management-hint">{t("该演员已在演员库，请更新信息或跳过。")}</p>}<div className="actor-scan-conflict-actions"><button className="actor-management-primary" type="button" disabled={busy || !current.canCreate} onClick={() => resolve(null, true)}>{t("新增演员")}</button><button className="actor-management-action" type="button" disabled={busy} onClick={() => resolve(null, false)}>{t("跳过")}</button></div></div>}
    {resolving && batch && <div className="actor-scan-conflict"><h3>{t("这位演员有多份资料，请选一份")}</h3><p className="actor-management-hint">{t("选择一份资料用于导入。")}</p><div className="actor-scan-conflict-actions">{batch.map(item => <button className="actor-management-action" key={item.key} type="button" disabled={busy} onClick={() => chooseBatch(item.key)}>{t("使用这份资料：{0}", [item.directory])}</button>)}<button className="actor-management-action" type="button" disabled={busy} onClick={() => chooseBatch(null)}>{t("全部跳过")}</button></div></div>}
    {preview && found > 0 && !resolving && <p className="actor-management-hint">{t("完整身份相同的演员将更新资料，未匹配的演员将新增。")}</p>}
    {relationships && <div className="actor-relationship-preview"><p className="actor-management-hint">{t("出演关系：可关联 {0}· 已有 {1}· 未找到 {2}", [relationships.available, relationships.existing, relationships.notFound])}</p>{!notice && <p className="actor-management-hint">{t("确认后补充可关联的影片，保留已有出演关系。")}</p>}{relationships.unmatched.length > 0 && <details><summary>{t("查看未找到的影片")}</summary><ul>{relationships.unmatched.map((film, index) => <li key={index}><strong>{film.actorName} · {film.folderName}</strong><ul>{film.videoFiles.map(file => <li key={file}>{file}</li>)}</ul><p>{t(film.reason)}</p></li>)}</ul></details>}</div>}
    {busy && <p className="actor-management-hint" role="status">{t("正在处理演员资料…")}</p>}{error && <p className="actor-management-error" role="alert">{t(error)}</p>}{notice && <p className="actor-management-notice" role="status">{t(notice)}</p>}
    <div className="actor-management-footer"><button className="actor-management-action" type="button" disabled={busy} onClick={onClose}>{notice ? t("完成") : t("取消")}</button>{!notice && !resolving && <button className="actor-management-primary" type="button" disabled={busy || !found} onClick={begin}>{current || batch ? t("继续") : t("确认添加")}</button>}</div>
  </ActorManagementDialog>
}
