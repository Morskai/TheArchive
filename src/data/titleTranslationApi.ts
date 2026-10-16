import { invoke } from './diagnostics.ts'
import type { TranslationJobStatus, TranslationPreview, TranslationProvider, TranslationSettings, TranslationSettingsState } from './titleTranslation'
export const loadTranslationSettings = () => invoke<TranslationSettingsState>('load_translation_settings')
export const saveTranslationSettings = (settings: TranslationSettings) => invoke<TranslationSettingsState>('save_translation_settings', { settings })
export const saveTranslationKey = (provider: TranslationProvider, key: string) => invoke<void>('save_translation_key', { provider, key })
export const loadTranslationKey = (provider: TranslationProvider) => invoke<string | null>('load_translation_key', { provider })
export const deleteTranslationKey = (provider: TranslationProvider) => invoke<void>('delete_translation_key', { provider })
export const testTranslationConnection = (provider: TranslationProvider) => invoke<void>('test_translation_connection', { provider })
export const previewTitleTranslations = () => invoke<TranslationPreview>('preview_title_translations')
export const startTitleTranslation = (filmIds: string[], force: boolean) => invoke<TranslationJobStatus>('start_title_translation', { filmIds, force })
export const translationJobStatus = () => invoke<TranslationJobStatus>('translation_job_status')
export const cancelTitleTranslation = () => invoke<TranslationJobStatus>('cancel_title_translation')
export const clearTranslationMemory = (expectedCount: number) => invoke<number>('clear_translation_memory', { expectedCount })

export const editFilmTitle = (filmId: string, expectedOriginal: string, expectedTitle: string, text: string) => invoke<import('./realFilms').RealFilm>('edit_film_title', { filmId, expectedOriginal, expectedTitle, text })
