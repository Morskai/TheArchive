import simplified from './zh-CN.ts'
import traditional from './zh-TW.ts'
import english from './en.ts'
import japanese from './ja.ts'
import type { Language } from './core.ts'
import { extraCatalog } from './extra.ts'

export const catalogs: Record<Language, Readonly<Record<string, string>>> = {
  'zh-CN': { ...simplified, ...extraCatalog('zh-CN') }, 'zh-TW': { ...traditional, ...extraCatalog('zh-TW') }, en: { ...english, ...extraCatalog('en') }, ja: { ...japanese, ...extraCatalog('ja') },
}
