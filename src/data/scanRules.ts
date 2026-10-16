import { invoke } from './diagnostics.ts'

export function normalizeIgnoredNames(names: string[]): string[] {
  if (names.length > 100) throw new Error('最多添加 100 个目录名称。')
  const seen = new Set<string>()
  return names.map(value => {
    const name = value.trim()
    if (!name || name === '.' || name === '..' || name.endsWith('.') || name.length > 255 || /[\u0000-\u001f\u007f-\u009f\\/:*?"<>|]/.test(name) || /^(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(\.|$)/i.test(name)) {
      throw new Error('请输入有效的文件夹名称，不能填写路径或通配符。')
    }
    const key = name.toLowerCase()
    if (seen.has(key)) throw new Error(`目录名称“${name}”重复，请合并条目。`)
    seen.add(key)
    return name
  })
}

export const loadIgnoredNames = () => invoke<string[]>('load_ignored_directory_names')
export const saveIgnoredNames = (names: string[]) => invoke<void>('save_ignored_directory_names', { names: normalizeIgnoredNames(names) })
