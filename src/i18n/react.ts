import { useSyncExternalStore } from 'react'
import { getLanguage, languageStore } from './language.ts'

export const useLocale = () => useSyncExternalStore(languageStore.subscribe, getLanguage, getLanguage)
export const useLanguageSettings = () => useSyncExternalStore(languageStore.subscribe, languageStore.getSnapshot, languageStore.getSnapshot)
export { t, message, languageStore, getLanguage } from './language.ts'
export type { UiText, Message, Language } from './core.ts'
