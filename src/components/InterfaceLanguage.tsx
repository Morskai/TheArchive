import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { isTauri } from '@tauri-apps/api/core'
import { AppSelect } from './AppSelect'
import { getLanguage, languageStore, t, useLanguageSettings, useLocale } from '../i18n/react'
import type { Language } from '../i18n/core'

export function InterfaceLanguageSync() {
  const locale = useLocale()
  useEffect(() => { document.documentElement.lang = locale }, [locale])
  useEffect(() => {
    if (!isTauri()) return
    let active = true
    let dispose: (() => void) | undefined
    // Register first: a restore or a change in the other window wins over an old read.
    void listen<string>('interface-language-changed', event => { if (active) languageStore.accept(event.payload) })
      .then(unlisten => {
        if (!active) { unlisten(); return }
        dispose = unlisten
        void languageStore.load()
      }).catch(() => { if (active) void languageStore.load() })
    const onFocus = () => { if (active) void languageStore.load() }
    window.addEventListener('focus', onFocus)
    return () => { active = false; dispose?.(); window.removeEventListener('focus', onFocus) }
  }, [])
  return null
}

const languageNames: Record<Language, string> = { ja: '日本語', en: 'English', 'zh-CN': '简体中文', 'zh-TW': '繁體中文' }
export function InterfaceLanguageSetting() {
  const state = useLanguageSettings()
  return <div className="settings-language">
    <div className="settings-preference"><div><strong>{t('界面语言')}</strong><span>{t('选择软件界面使用的语言。')}</span></div>
      <AppSelect className="settings-content-select" popupSizing="content" aria-label={t('界面语言')} value={state.language} disabled={!state.ready || state.busy} onChange={event => { void languageStore.save(event.target.value as Language) }}>
        {(Object.keys(languageNames) as Language[]).map(language => <option key={language} value={language} lang={language}>{languageNames[language]}</option>)}
      </AppSelect>
    </div>
    {state.busy && <p className="settings-backup-feedback" role="status">{t(state.ready ? '正在保存语言设置…' : '正在读取语言设置…')}</p>}
    {state.error && <p className="settings-backup-feedback settings-error" role="alert">{t(state.error === 'load' ? '语言设置读取失败，请重试。' : '语言设置未保存，请重试。')}{state.error === 'load' && <button className="settings-link" type="button" onClick={() => { void languageStore.load() }}>{t('重新读取')}</button>}</p>}
  </div>
}

export const currentDateLocale = () => getLanguage() === 'en' ? 'en-US' : getLanguage()
