import { t, useLocale } from '../i18n/react'
import { useRef, useState } from 'react'
import { invoke } from '../data/diagnostics'

export function ConfigurationDirectory({ path, desktop }: { path: string; desktop: boolean }) {
  useLocale()
  const displayPath = path === '暂时无法读取' || path === '正在读取…' ? t(path) : path
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const guard = useRef(false)
  const open = async () => {
    if (guard.current || !desktop) return
    guard.current = true; setBusy(true); setError('')
    try { await invoke('open_application_data_directory') }
    catch { setError('配置目录未能打开，请重试。') }
    finally { guard.current = false; setBusy(false) }
  }
  return <div className="settings-config-path"><span>{t("配置目录")}</span><button type="button" aria-label={t("打开配置目录")} title={displayPath} disabled={!desktop || busy} onClick={() => { void open() }}><code>{displayPath}</code></button>{error && <span className="settings-config-error" role="alert">{t(error)}</span>}</div>
}
