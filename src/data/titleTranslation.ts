import type { RealFilm } from './realFilms'
import { filmDisplayCode } from './realFilmPresentation.ts'
export type TranslationProvider = 'deepl' | 'google' | 'azure' | 'baidu'
export interface TranslationSettings { enabled: boolean; provider: TranslationProvider; deepl_plan: 'free' | 'pro'; target_language: 'ZH-HANS'; azure_region: string; baidu_app_id: string }
export interface TranslationStats { pending: number; automatic: number; memory: number; local_reuse: number; stale: number; api_requests: number }
export interface TranslationSettingsState { settings: TranslationSettings; key_configured: boolean; stats: TranslationStats }
export interface TranslationEntry { film_id: string; original_title: string; title?: string; sources?: { id: string; name: string }[]; body: string; status: 'chinese' | 'manual' | 'automatic' | 'local_memory' | 'stale' | 'pending' | 'language_uncertain' }
export interface TranslationPreview { summary: TranslationStats; entries: TranslationEntry[] }
export interface TranslationJobStatus { running: boolean; cancelled: boolean; total: number; completed: number; succeeded: number; failed: number; reused: number; api_requests: number; last_error: string | null }
export function selectedTranslationEntries(entries: TranslationEntry[], ids: string[], force: boolean) { const selected = new Set(ids); return entries.filter(entry => selected.has(entry.film_id) && (force ? entry.status === 'automatic' || entry.status === 'stale' : entry.status === 'pending' || entry.status === 'local_memory' || entry.status === 'language_uncertain')) }
export function defaultTranslationIds(entries: TranslationEntry[], force: boolean) {
  return selectedTranslationEntries(entries, entries.map(entry => entry.film_id), force).filter(entry => entry.status !== 'language_uncertain').map(entry => entry.film_id)
}
export function selectedTranslationCounts(entries: TranslationEntry[], ids: string[], force: boolean) {
  const selected = selectedTranslationEntries(entries, ids, force)
  return { candidates: selected.length, localReuse: selected.filter(entry => entry.status === 'local_memory' && !force).length, apiRequests: new Set(selected.filter(entry => force || entry.status !== 'local_memory').map(entry => entry.body)).size }
}
export type TranslationStatusFilter = 'all' | 'translated' | 'uncertain'
export function filterTranslationEntries(entries: TranslationEntry[], query: string, source: string, status: TranslationStatusFilter = 'all') {
  const term = query.trim().toLocaleLowerCase()
  return entries.filter(entry => (source === 'all' || entry.sources?.some(item => item.id === source))
    && (status === 'all' || (status === 'translated' ? entry.status === 'manual' || entry.status === 'automatic' || entry.status === 'stale' : entry.status === 'language_uncertain'))
    && (!term || [entry.original_title, entry.title].some(title => title?.toLocaleLowerCase().includes(term))))
}
export function toggleVisibleTranslationIds(selected: string[], visible: string[]) {
  const current = new Set(selected), ids = new Set(visible)
  if (visible.length && visible.every(id => current.has(id))) return selected.filter(id => !ids.has(id))
  return [...new Set([...selected, ...visible])]
}
export function titleSearchText(film: RealFilm) { return [film.title, film.original_title, film.automatic_title, filmDisplayCode(film), ...(film.actors ?? []).map(actor => actor.name)].filter(Boolean).join(' ') }
export function translationError(error: unknown) {
  if (error === 'translation_changed' || error === 'translation_source_changed') return '影片片名或原始目录已变化，请重新打开详情后编辑。'
  if (error === 'translation_save_failed') return '片名未保存，原片名已保留，请重试。'
  if (error === 'translation_result_invalid') return '片名内容无效，请输入单行文字。'
  if (error === 'translation_memory_changed') return '翻译记忆数量已变化，请重新检测后确认。'
  if (error === 'translation_app_id_missing') return '请填写并保存百度 APP ID，再测试连接或翻译。'
  if (error === 'translation_invalid_region' || error === 'translation_settings_invalid') return '翻译设置格式无效，请检查 Azure 区域和百度 APP ID。'
  if (error === 'translation_text_too_large') return '标题正文超出当前服务的单次请求限制，该影片未翻译。'
  if (error === 'translation_key_missing') return '请先为当前服务配置密钥，再测试连接或翻译。'
  if (error === 'translation_disabled') return '请先启用并保存影片标题翻译。'
  if (error === 'translation_job_running' || error === 'translation_busy') return '已有翻译任务正在执行，请等待或取消后重试。'
  if (error === 'translation_auth_failed' || error === 'translation_auth') return '服务拒绝了认证，请更换密钥后测试连接。'
  if (error === 'translation_rate_limited' || error === 'translation_rate_limit' || error === 'translation_quota') return '服务额度或请求频率受限，请稍后再试。'
  if (error === 'protection_failed') return '软件资料保护失败，未执行本次操作，请检查软件资料目录后重试。'
  if (error === 'translation_network' || error === 'translation_timeout' || error === 'translation_server') return '翻译服务暂时无法连接；已完成结果保留，请稍后重试。'
  if (error === 'translation_invalid_response' || error === 'translation_response_too_large') return '翻译服务返回的内容无法使用；已有标题保留，请稍后重试。'
  if (error === 'translation_credentials_error' || error === 'translation_credentials_unsupported') return 'Windows 安全凭据存储暂时无法使用，请检查系统后重试。'
  if (error === 'translation_source_changed') return '影片原始标题已变化，请重新检测后重试。'
  return '操作未完成，请稍后重试。'
}
