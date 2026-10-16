import { ignoredNameMessage } from '../i18n/feedback'
import { t, useLocale, type UiText } from '../i18n/react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { loadIgnoredNames, normalizeIgnoredNames, saveIgnoredNames } from '../data/scanRules'
import { nativeSourcesAvailable } from '../data/sources'

export function IgnoredDirectoryNames({ disabled }: { disabled: boolean }) {
  useLocale()
  const [open, setOpen] = useState(false)
  const [saved, setSaved] = useState(false)
  const trigger = useRef<HTMLButtonElement>(null)
  const close = useCallback((didSave = false) => { setOpen(false); setSaved(didSave); trigger.current?.focus() }, [])
  return <>
    <button ref={trigger} type="button" className="settings-action" disabled={disabled || !nativeSourcesAvailable} onClick={() => { setSaved(false); setOpen(true) }}>{t("添加忽略目录")}</button>
    {saved && <span className="settings-ignore-saved" role="status">{t("已保存，下次扫描生效。")}</span>}
    {open && createPortal(<IgnoreNamesDialog onClose={close} />, document.body)}
  </>
}

function IgnoreNamesDialog({ onClose }: { onClose: (saved?: boolean) => void }) {
  useLocale()
  const dialog = useRef<HTMLDialogElement>(null)
  const input = useRef<HTMLInputElement>(null)
  const sequence = useRef(0)
  const saving = useRef(false)
  const [rows, setRows] = useState<{ id: number; name: string }[]>([])
  const [name, setName] = useState('')
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<UiText>('')
  const [retry, setRetry] = useState(0)
  const [loaded, setLoaded] = useState(false)
  const close = (saved = false) => { dialog.current?.close(); onClose(saved) }

  useEffect(() => {
    const element = dialog.current
    element?.showModal()
    return () => element?.close()
  }, [])
  useEffect(() => {
    let active = true
    loadIgnoredNames().then(names => {
      if (!active) return
      setRows(names.map(value => ({ id: sequence.current++, name: value })))
      setLoaded(true); setError(''); input.current?.focus()
    }).catch(() => { if (active) setError('忽略目录名称暂时无法读取，请重试。') })
      .finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [retry])
  useEffect(() => { if (loaded && !loading) input.current?.focus() }, [loaded, loading])

  const add = () => {
    try {
      const normalized = normalizeIgnoredNames([...rows.map(row => row.name), name])
      setRows(current => [...current, { id: sequence.current++, name: normalized.at(-1)! }])
      setName(''); setError(''); input.current?.focus()
    } catch (reason) { setError(ignoredNameMessage(reason)) }
  }
  const save = async () => {
    if (saving.current || !loaded) return
    try { normalizeIgnoredNames(rows.map(row => row.name)) }
    catch (reason) { setError(ignoredNameMessage(reason)); return }
    if (name.trim()) { setError('新名称还未添加，请先点击“添加”。'); return }
    saving.current = true; setBusy(true); setError('')
    try { await saveIgnoredNames(rows.map(row => row.name)); close(true) }
    catch { setError('忽略目录名称未保存，请重试。') }
    finally { saving.current = false; setBusy(false) }
  }

  return <dialog data-app-button-surface ref={dialog} className="ignore-names-dialog" aria-labelledby="ignore-names-title" aria-describedby="ignore-names-help" onCancel={event => { event.preventDefault(); if (!saving.current) close() }}>
    <div className="ignore-names-head"><h2 id="ignore-names-title">{t("忽略目录名称")}</h2><button type="button" className="close-button dialog-close-button" disabled={busy} onClick={() => close()} aria-label={t("关闭忽略目录名称")}>×</button></div>
    <p id="ignore-names-help">{t("所有片库按完整文件夹名称匹配，忽略大小写。匹配的目录及其内容跳过扫描。")}</p>
    {loading ? <p role="status">{t("正在读取…")}</p> : loaded ? <>
      <form className="ignore-names-add" onSubmit={event => { event.preventDefault(); add() }}><label htmlFor="ignore-new-name">{t("添加目录名称")}</label><div><input id="ignore-new-name" ref={input} value={name} onChange={event => { setName(event.target.value); setError('') }} placeholder={t("例如：写真")} maxLength={255} disabled={busy} /><button type="submit" className="settings-action" disabled={busy || rows.length >= 100}>{t("添加")}</button></div></form>
      <div className="ignore-names-list-heading">{t("已忽略名称")}<span>{rows.length}</span></div>
      {rows.length ? <ul className="ignore-names-list">{rows.map((row, index) => <li key={row.id}><input aria-label={t("忽略目录名称 {0}", [index + 1])} value={row.name} maxLength={255} disabled={busy} onChange={event => { const value = event.target.value; setRows(current => current.map(item => item.id === row.id ? { ...item, name: value } : item)); setError('') }} /><button type="button" className="settings-action settings-danger" disabled={busy} aria-label={t("删除忽略名称 {0}", [row.name])} onClick={() => { setRows(current => current.filter(item => item.id !== row.id)); setError('') }}>{t("删除")}</button></li>)}</ul> : <p className="ignore-names-empty">{t("暂无忽略名称。")}</p>}
    </> : <button type="button" className="settings-action" onClick={() => { setLoading(true); setError(''); setRetry(current => current + 1) }}>{t("重新读取")}</button>}
    {error && <p className="settings-error" role="alert">{t(error)}</p>}
    <p className="ignore-names-footnote">{t("保存后，下次手动扫描生效。")}</p>
    <div className="ignore-names-footer"><button type="button" className="settings-action" disabled={busy} onClick={() => close()}>{t("取消")}</button><button type="button" className="settings-primary" disabled={busy || loading || !loaded} onClick={() => void save()}>{busy ? t("正在保存…") : t("保存")}</button></div>
  </dialog>
}
