import { isTauri } from '@tauri-apps/api/core'
import { invoke } from '../data/diagnostics.ts'
import { createLanguageStore } from './store.ts'
import { catalogs } from './messages.ts'
import { formatMessage, type MessageValue, type UiText } from './core.ts'

const desktop = isTauri()
let previewLanguage = 'zh-CN'
export const languageStore = createLanguageStore({
  load: () => desktop ? invoke<unknown>('load_interface_language') : Promise.resolve(previewLanguage),
  save: async language => { if (desktop) await invoke<void>('save_interface_language', { language }); else previewLanguage = language },
}, !desktop)
export const getLanguage = () => languageStore.getSnapshot().language
export const t = (text: UiText, values?: readonly MessageValue[]) => formatMessage(catalogs, getLanguage(), text, values)
export { message, type UiText, type Message, type Language } from './core.ts'
