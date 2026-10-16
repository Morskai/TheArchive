export function editText(value: string, start: number, end: number, inserted: string, maxLength = -1) {
  const allowed = maxLength < 0 ? inserted : inserted.slice(0, Math.max(0, maxLength - (value.length - (end - start))))
  return { value: value.slice(0, start) + allowed + value.slice(end), caret: start + allowed.length }
}
