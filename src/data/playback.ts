import type { VideoResolution } from './deliveryPresentation'
import { invoke } from './diagnostics.ts'
import { filmManualReads, filmManualSaved } from './manualEvents'
import { favoriteReads } from './filmFavoriteEvents'
import type { PreviewSettings } from './settings'
import type { RealFilm } from './realFilms'
import type { LaunchEvent } from './history'

export type PlayerOptions = Pick<PreviewSettings, 'playerMode' | 'playerPath' | 'launchArguments' | 'internalDisplay'>
export interface RealHistory { films: RealFilm[]; events: LaunchEvent[]; film_count: number; source_count: number }
export const loadPlayerOptions = async () => { const options = await invoke<PlayerOptions>('load_player_settings'); return { ...options, playerMode: options.playerMode === 'custom' ? 'custom' as const : 'internal' as const, internalDisplay: options.internalDisplay ?? 'overlay' } }
export const loadApplicationDataDirectory = () => invoke<string>('application_data_directory')
export const savePlayerOptions = (settings: PlayerOptions) => invoke<void>('save_player_settings', { settings })
export const loadLaunchHistory = () => favoriteReads.read(() => filmManualReads.read(() => invoke<RealHistory>('load_launch_history'), (history, patch) => ({ ...history, films: patch(history.films) })), (history, patch) => ({ ...history, films: patch(history.films) }))
export const clearLaunchHistory = () => invoke<void>('clear_launch_history')
export const openFilmFolder = (filmId: string, videoId: string) => invoke<void>('open_film_folder', { filmId, videoId })
export const loadVideoSize = (filmId: string, videoId: string) => invoke<number>('load_video_size', { filmId, videoId })
export const loadVideoResolution = (filmId: string, videoId: string) => invoke<VideoResolution | null>('load_video_resolution', { filmId, videoId })
export interface DirectoryFacts { path: string; videos: number; images: number; subtitles: number }
export const loadDirectoryFacts = (filmId: string, videoId: string) => invoke<DirectoryFacts>('load_directory_facts', { filmId, videoId })
export const setPreferredVideo = async (filmId: string, videoId: string, preferred: boolean) => { await invoke<void>('set_preferred_video', { filmId, videoId, preferred }); filmManualSaved(filmId, { preferred_video_id: preferred ? videoId : null }) }
export const notifyRealLibraryChanged = () => window.dispatchEvent(new Event('real-library-changed'))

// Shared across drawer unmounts: a still-pending click cannot start a second player.
const pending = new Map<string, Promise<{ historySaved: boolean }>>()
export function launchFilmVideo(filmId: string, videoId: string, playerToken?: number) {
  const key = `${filmId}/${videoId}/${playerToken ?? ''}`
  const existing = pending.get(key)
  if (existing) return existing
  const request = invoke<{ historySaved: boolean }>('launch_film_video', { filmId, videoId, playerToken, requestId: crypto.randomUUID() })
    .finally(() => pending.delete(key))
  pending.set(key, request)
  return request
}

const messages: Record<string, string> = {
  internal_player_unavailable: '内置播放引擎无法加载，请重新安装完整安装包。',
  player_window_failed: '播放窗口未能打开，请重试。',
  player_unavailable: '播放器暂时无法响应，请关闭后重新播放。',
  player_replaced: '播放内容已切换，请在当前播放器中操作。',
  media_unplayable: '此视频无法播放，请检查文件或改用外部播放器。',
  player_load_timeout: '视频加载超时，请检查磁盘连接后重试。',
  player_action_failed: '播放操作未完成，请重试。',
  subtitle_unavailable: '字幕无法读取，请重新选择字幕文件。',
  invalid_program: '请填写本机播放器 .exe 程序的完整路径。',
  program_unavailable: '播放器程序不存在或无法读取，请检查程序路径。',
  invalid_arguments: '启动参数的引号未配对或内容无效，请检查。',
  invalid_mode: '请选择内置或自定义播放器。',
  settings_load_failed: '播放器选项暂时无法读取，请重试。',
  settings_save_failed: '播放器选项未保存，请重试。',
  source_removed: '此文件的来源已移除，请先在设置中重新添加该来源。',
  source_paused: '此文件的来源已停用，请先在设置中启用。',
  root_unavailable: '片库目录无法访问，请检查磁盘连接与读取权限。',
  file_unavailable: '所选文件不存在或无法读取，请检查磁盘与文件权限。',
  directory_unavailable: '目录无法完整读取，请检查访问权限。',
  video_not_linked: '所选文件未关联到当前影片，请重新打开详情。',
  path_outside_source: '所选文件不在绑定来源的范围内，无法打开。',
  no_file_association: 'Windows 尚未为此类视频设置默认播放器。',
  access_denied: 'Windows 拒绝了打开请求，请检查访问权限。',
  launch_failed: 'Windows 未接受打开请求，请检查播放器程序或默认应用设置。',
  launch_result_unknown: '无法确认启动结果，请先检查播放器和历史记录，软件不会自动再次启动。',
  preferred_save_failed: '首选版本未保存，请重试。',
  history_load_failed: '启动记录暂时无法读取，请重试。',
  history_clear_failed: '历史未清除，请重试。',
  protection_failed: '应用资料保护备份失败，历史未清除。请检查应用数据目录。',
}
export const playbackError = (error: unknown) => messages[String(error)] ?? '操作未完成，请检查当前状态后重试。'
