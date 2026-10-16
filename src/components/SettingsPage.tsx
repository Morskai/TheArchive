import { t, message as uiMessage, useLocale, type UiText } from '../i18n/react'
import { AppSelect } from './AppSelect'
import { invoke } from '@tauri-apps/api/core'
import { ApplicationOptions } from './ApplicationOptions'
import { InterfaceLanguageSetting } from './InterfaceLanguage'
import { memo, useEffect, useRef, useState } from 'react'
import { loadInterfacePreferences, saveInterfacePreferences, type PreviewSettings, type InterfacePreferences } from '../data/settings'
import { BackupSection } from './BackupSection'
import { TitleTranslationSection } from './TitleTranslationSection'
import { MaintenanceSection } from './MaintenanceSection'
import { DiagnosticSection } from './DiagnosticSection'
import { SourceScanSection } from './SourceScanSection'
import '../settings.css'
import { realFilmsAvailable } from '../data/realFilms'
import { loadPlayerOptions, savePlayerOptions, playbackError } from '../data/playback'

interface SettingsPageProps {
  settings: PreviewSettings
  onUpdateSettings: (patch: Partial<PreviewSettings>) => void
  onDataCleared: () => void
  scanGuideActive?: boolean
}

export const SettingsPage = memo(function SettingsPage({ settings, onUpdateSettings, onDataCleared, scanGuideActive = false }: SettingsPageProps) {
  useLocale()
  const [preferencesLoaded, setPreferencesLoaded] = useState(!realFilmsAvailable)
  const [preferencesBusy, setPreferencesBusy] = useState(realFilmsAvailable)
  const [preferencesError, setPreferencesError] = useState('')
  const [preferencesNotice, setPreferencesNotice] = useState('')
  const preferencesGuard = useRef(false)
  const [preferencesRead, setPreferencesRead] = useState(0)
  useEffect(() => {
    if (!realFilmsAvailable) return
    let active = true
    setPreferencesBusy(true); setPreferencesError('')
    loadInterfacePreferences().then(preferences => {
      if (active) { onUpdateSettings(preferences); setPreferencesLoaded(true) }
    }).catch(() => { if (active) setPreferencesError('界面偏好暂时无法读取，请重试。') })
      .finally(() => { if (active) setPreferencesBusy(false) })
    return () => { active = false }
  }, [onUpdateSettings, preferencesRead])

  const updatePreferences = async (patch: Partial<InterfacePreferences>) => {
    if (preferencesGuard.current || !preferencesLoaded) return
    if (!realFilmsAvailable) { onUpdateSettings(patch); setPreferencesNotice('本次预览已应用；桌面版保存后可在重开时保留。'); return }
    preferencesGuard.current = true
    setPreferencesBusy(true); setPreferencesError(''); setPreferencesNotice('')
    const preferences: InterfacePreferences = { reduceMotion: settings.reduceMotion, thinScrollbar: settings.thinScrollbar, filmCoverRatio: settings.filmCoverRatio, actorPhotoRatio: settings.actorPhotoRatio, filmCoverFit: settings.filmCoverFit, actorPhotoFit: settings.actorPhotoFit, ...patch }
    try { await saveInterfacePreferences(preferences); onUpdateSettings(preferences); setPreferencesNotice('界面偏好已生效。') }
    catch { setPreferencesError('界面偏好未保存，已保留原设置，请重试。') }
    finally { preferencesGuard.current = false; setPreferencesBusy(false) }
  }
  const [playerDraft, setPlayerDraft] = useState(() => ({
    playerMode: settings.playerMode,
    playerPath: settings.playerPath,
    launchArguments: settings.launchArguments,
    internalDisplay: settings.internalDisplay,
  }))
  const [playerNotice, setPlayerNotice] = useState<UiText>('')
  const [playerBusy, setPlayerBusy] = useState(realFilmsAvailable)
  const playerGuard = useRef(false)
  const [playerLoaded, setPlayerLoaded] = useState(!realFilmsAvailable)
  const initialDisplay = useRef(settings.internalDisplay)
  const [restartAvailable, setRestartAvailable] = useState(false)
  const [restarting, setRestarting] = useState(false)
  useEffect(() => {
    if (!realFilmsAvailable) return
    let active = true
    loadPlayerOptions().then(options => { if (active) { initialDisplay.current = options.internalDisplay; setPlayerDraft(options); onUpdateSettings(options); setPlayerLoaded(true) } })
      .catch(error => { if (active) setPlayerNotice(playbackError(error)) })
      .finally(() => { if (active) setPlayerBusy(false) })
    return () => { active = false }
  }, [onUpdateSettings])

  const savePlayer = async () => {
    if (playerGuard.current || !playerLoaded || !realFilmsAvailable) return
    if (playerDraft.playerMode === 'custom' && !playerDraft.playerPath.trim()) {
      setPlayerNotice('请填写播放器程序路径。')
      return
    }
    const options = { ...playerDraft, playerPath: playerDraft.playerPath.trim() }
    playerGuard.current = true
    setPlayerBusy(true)
    setPlayerNotice('正在保存…')
    try { await savePlayerOptions(options); onUpdateSettings(options); setRestartAvailable(options.internalDisplay !== initialDisplay.current); setPlayerNotice('播放器选项已保存。') }
    catch (error) { setPlayerNotice(uiMessage("选项未保存：{0}", [uiMessage(playbackError(error))])) }
    finally { playerGuard.current = false; setPlayerBusy(false) }
  }

  const restart = async () => {
    if (playerGuard.current || restarting) return
    playerGuard.current = true; setRestarting(true)
    try { await invoke('restart_application') }
    catch { setPlayerNotice('软件未能重启，请重试。'); setRestarting(false); playerGuard.current = false }
  }

  return <section className="settings-page" aria-labelledby="settings-title">
    <div className="settings-heading"><div><h1 id="settings-title">{t("设置")}</h1><p>{t("管理片库目录、播放方式与界面偏好")}</p></div></div>

    <section id="settings-library-scan" className="settings-block settings-libraries" aria-labelledby="settings-libraries-title">
      <SourceScanSection guideActive={scanGuideActive} />
    </section>

    <div className="settings-pair">
      <section className="settings-block" aria-labelledby="settings-player-title">
        <div className="settings-block-head"><div><h2 id="settings-player-title">{t("播放器")}</h2><p>{realFilmsAvailable ? t("选择打开视频的方式") : t("请在桌面版设置播放器")}</p></div></div>
        <div className="settings-choice-group" role="radiogroup" aria-label={t("播放器方式")}>
          <label className={playerDraft.playerMode === 'internal' ? 'selected' : ''}><input type="radio" disabled={playerBusy} name="player-mode" value="internal" checked={playerDraft.playerMode === 'internal'} onChange={() => { setPlayerDraft(current => ({ ...current, playerMode: 'internal' })); setPlayerNotice('') }} /><span><strong>{t("内置播放器")}</strong><small>{t("在 TheArchive 中播放视频")}</small></span></label>
          <label className={playerDraft.playerMode === 'custom' ? 'selected' : ''}><input type="radio" disabled={playerBusy} name="player-mode" value="custom" checked={playerDraft.playerMode === 'custom'} onChange={() => { setPlayerDraft(current => ({ ...current, playerMode: 'custom' })); setPlayerNotice('') }} /><span><strong>{t("自定义播放器")}</strong><small>{t("指定本机播放器程序")}</small></span></label>
        </div>
        {playerDraft.playerMode === 'internal' && <div className="settings-fields"><label>{t("内置播放器显示方式")}<AppSelect className="settings-content-select" popupSizing="content" aria-label={t("内置播放器显示方式")} value={playerDraft.internalDisplay} disabled={playerBusy || restarting} onChange={event => { setPlayerDraft(current => ({ ...current, internalDisplay: event.target.value as 'overlay' | 'window' })); setPlayerNotice('') }}><option value="overlay">{t("覆盖主窗口")}</option><option value="window">{t("独立窗口")}</option></AppSelect></label></div>}
        {playerDraft.playerMode === 'custom' && <div className="settings-fields"><label>{t("程序路径")}<input type="text" disabled={playerBusy} value={playerDraft.playerPath} onChange={event => { setPlayerDraft(current => ({ ...current, playerPath: event.target.value })); setPlayerNotice('') }} placeholder={t("例如 D:\\Apps\\Player\\player.exe")} aria-label={t("播放器程序路径")} /></label><label>{t("启动参数")}<span>{t("可选")}</span><input type="text" disabled={playerBusy} value={playerDraft.launchArguments} onChange={event => { setPlayerDraft(current => ({ ...current, launchArguments: event.target.value })); setPlayerNotice('') }} placeholder={t("留空则使用播放器默认方式")} aria-label={t("播放器启动参数")} /></label></div>}
        <div className="settings-save-row"><button type="button" className="settings-primary" disabled={playerBusy || restarting || !realFilmsAvailable || !playerLoaded} onClick={savePlayer}>{t("保存播放器选项")}</button><span role="status" className={playerNotice && !['正在保存…', '播放器选项已保存。'].includes(typeof playerNotice === 'string' ? playerNotice : playerNotice.key) ? 'settings-error' : ''}>{playerBusy && !playerLoaded ? t("正在读取播放器选项…") : t(playerNotice)}</span>{restartAvailable && <button type="button" className="settings-action" disabled={playerBusy || restarting} onClick={restart}>{restarting ? t("正在重启…") : t("重启软件")}</button>}</div>
      </section>

      <section className="settings-block settings-appearance" aria-labelledby="settings-appearance-title">
        <div className="settings-block-head"><div><h2 id="settings-appearance-title">{t("界面")}</h2><p>{t("调整运行方式与界面显示")}</p></div></div>
        <InterfaceLanguageSetting />
        <ApplicationOptions />
        <div className="settings-preference"><div><strong>{t("减少动态效果")}</strong><span>{t("减少卡牌、悬停和页面切换动画；系统设置始终优先生效。")}</span></div><label className="settings-switch"><input type="checkbox" disabled={preferencesBusy || !preferencesLoaded} checked={settings.reduceMotion} onChange={event => void updatePreferences({ reduceMotion: event.target.checked })} aria-label={t("减少动态效果")} /><span aria-hidden="true" /></label></div>
        <div className="settings-preference"><div><strong>{t("细滚动条")}</strong><span>{t("使用更窄的页面滚动条。")}</span></div><label className="settings-switch"><input type="checkbox" disabled={preferencesBusy || !preferencesLoaded} checked={settings.thinScrollbar} onChange={event => void updatePreferences({ thinScrollbar: event.target.checked })} aria-label={t("细滚动条")} /><span aria-hidden="true" /></label></div>
        <div className="settings-image-pair"><div className="settings-preference settings-image-preference"><div><strong>{t("影片封面显示")}</strong><span>{t("选择图片在封面框中的显示方式。")}</span></div><div className="settings-image-controls">
          <AppSelect className="settings-content-select" popupSizing="content" aria-label={t("影片封面显示方式")} disabled={preferencesBusy || !preferencesLoaded} value={settings.filmCoverFit} onChange={event => void updatePreferences({ filmCoverFit: event.target.value as InterfacePreferences['filmCoverFit'] })}><option value="cover">{t("铺满裁切")}</option><option value="contain">{t("完整显示")}</option></AppSelect>
        </div></div>
        <div className="settings-preference settings-image-preference"><div><strong>{t("演员照片显示")}</strong><span>{t("选择照片在图片框中的显示方式。")}</span></div><div className="settings-image-controls">
          <AppSelect className="settings-content-select" popupSizing="content" aria-label={t("演员照片显示方式")} disabled={preferencesBusy || !preferencesLoaded} value={settings.actorPhotoFit} onChange={event => void updatePreferences({ actorPhotoFit: event.target.value as InterfacePreferences['actorPhotoFit'] })}><option value="cover">{t("铺满裁切")}</option><option value="contain">{t("完整显示")}</option></AppSelect>
        </div></div>
        </div>
        {preferencesBusy && <p className="settings-backup-feedback" role="status">{preferencesLoaded ? t("正在保存界面偏好…") : t("正在读取界面偏好…")}</p>}
        {preferencesNotice && <p className="settings-backup-feedback" role="status">{t(preferencesNotice)}</p>}
        {preferencesError && <p className="settings-backup-feedback settings-error" role="alert">{t(preferencesError)}{!preferencesLoaded && <button type="button" className="settings-link" disabled={preferencesBusy} onClick={() => setPreferencesRead(value => value + 1)}>{t("重新读取")}</button>}</p>}
      </section>
    </div>
    <TitleTranslationSection />
    <div className="settings-backup-row"><BackupSection /></div>
    <MaintenanceSection onDataCleared={onDataCleared} />
    <DiagnosticSection />
  </section>
})
