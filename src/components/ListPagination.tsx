import { t, useLocale } from '../i18n/react'
import { useEffect, useId, useState } from 'react'
import '../list-pagination.css'

function paginationRange(current: number, total: number): Array<number | 'ellipsis'> {
  const pages = [...new Set([1, 2, current - 1, current, current + 1, total - 1, total]
    .filter(page => page >= 1 && page <= total))].sort((a, b) => a - b)
  return pages.flatMap((page, index) => index > 0 && page - pages[index - 1] > 1 ? ['ellipsis' as const, page] : [page])
}

export function ListPagination({ label, total, pageSize, page, unit = '条', disabled = false, resetKey, onChange }: {
  label: string; total: number; pageSize: number; page: number; unit?: string; disabled?: boolean; resetKey: string; onChange: (page: number) => void
}) {
  useLocale()
  const [input, setInput] = useState('')
  const inputId = useId()
  const pageCount = Math.max(1, Math.ceil(total / pageSize))
  useEffect(() => { setInput('') }, [page, resetKey])
  const changePage = (next: number) => { if (disabled) return; setInput(''); onChange(next) }
  return <nav className="list-pagination" aria-label={label}>
    <span className="list-pagination-summary">{t(unit === '个对象' ? "共 {0} 个对象 · 每页 {1} 个对象" : "共 {0} 条 · 每页 {1} 条", [total, pageSize])}</span>
    <div className="list-pagination-controls">
      <button type="button" disabled={disabled || page === 1} onClick={() => changePage(page - 1)}>{t("上一页")}</button>
      <div className="list-page-numbers">{paginationRange(page, pageCount).map((item, index) => item === 'ellipsis' ? <span key={`ellipsis-${index}`} aria-hidden="true">…</span> : <button key={item} type="button" disabled={disabled} aria-label={t("第 {0} 页", [item])} aria-current={page === item ? 'page' : undefined} className={page === item ? 'active' : ''} onClick={() => changePage(item)}>{item}</button>)}</div>
      <button type="button" disabled={disabled || page === pageCount} onClick={() => changePage(page + 1)}>{t("下一页")}</button>
    </div>
    <form className="list-page-jump" onSubmit={event => { event.preventDefault(); const next = Number(input); if (Number.isInteger(next) && next >= 1 && next <= pageCount) changePage(next) }}>
      <label htmlFor={inputId}>{t("跳至")}</label><input id={inputId} type="number" min="1" max={pageCount} value={input} disabled={disabled} onChange={event => setInput(event.target.value)} aria-label={t("跳转页码")} /><span>{t("页")}</span><button type="submit" disabled={disabled}>{t("跳转")}</button>
    </form>
  </nav>
}
