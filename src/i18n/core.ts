export const languages = ['zh-CN', 'zh-TW', 'en', 'ja'] as const
export type Language = typeof languages[number]
export const isLanguage = (value: unknown): value is Language => languages.includes(value as Language)

export interface Message { readonly key: string; readonly values: readonly MessageValue[] }
export type MessageValue = string | number | boolean | null | undefined | Message
export type UiText = string | Message
export const message = (key: string, values: readonly MessageValue[] = []): Message => ({ key, values })

// Only software-owned text is resolved here. Interpolated user content is literal,
// including braces, dollar signs and strings that happen to match a catalog key.
export function formatMessage(catalogs: Record<string, Readonly<Record<string, string>>>, language: string, text: UiText, values: readonly MessageValue[] = []): string {
  const key = typeof text === 'string' ? text : text.key
  const args = typeof text === 'string' ? values : text.values
  const template = catalogs[language]?.[key] ?? key
  return template.replace(/\{(\d+)\}/g, (placeholder, index: string) => {
    if (Number(index) >= args.length) return placeholder
    const value = args[Number(index)]
    return typeof value === 'object' && value !== null ? formatMessage(catalogs, language, value) : String(value ?? '')
  })
}
