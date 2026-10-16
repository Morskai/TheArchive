import { t, useLocale } from '../i18n/react'
import { activePlaybackDays } from '../data/deliveryPresentation'
import { AppSelect } from './AppSelect'
import { ListPagination } from './ListPagination'
import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowIcon } from './ui'
import { FilmArtwork, type FilmCardView } from './FilmCard'
import type { LaunchEvent } from '../data/history'
import { eventDate, localDateKey, localLaunchTime } from '../data/localDates'
import '../playback-history.css'

interface PlaybackHistoryProps {
  films: Array<FilmCardView & { actor: string; library?: string }>
  events: LaunchEvent[]
  loading: boolean
  onClear: () => void | Promise<void>
  onOpenFilm: (id: string) => void
  real?: boolean
  error?: string
  active?: boolean
  onRetry?: () => void
  onActivityDays?: (count: number | null) => void
}

const pageSize = 10

function dayKey(daysAgo: number) {
  const date = new Date()
  date.setDate(date.getDate() - daysAgo)
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`
}

export function PlaybackHistory({ films, events, loading, onClear, onOpenFilm, real = false, error = '', active = true, onRetry, onActivityDays }: PlaybackHistoryProps) {
  useLocale()
  const [query, setQuery] = useState('')
  const [sort, setSort] = useState<'time' | 'title'>('time')
  const [direction, setDirection] = useState<'asc' | 'desc'>('desc')
  const [page, setPage] = useState(1)
  const [confirmClear, setConfirmClear] = useState(false)
  const [clearBusy, setClearBusy] = useState(false)
  const [clearError, setClearError] = useState('')
  const clearGuard = useRef(false)
  useEffect(() => { if (!active) setConfirmClear(false) }, [active])
  const filmById = useMemo(() => new Map(films.map(film => [film.id, film])), [films])
  const records = useMemo(() => events.flatMap(event => {
    const film = filmById.get(event.filmId)
    return film ? [{ event, film, timestamp: real ? eventDate(event.launchedAt).getTime() : null }] : []
  }), [events, filmById, real])
  const visible = useMemo(() => {
    const term = query.trim().toLocaleLowerCase()
    return records.filter(({ film }) => !term || [film.title, film.code, film.actor].some(value => value.toLocaleLowerCase().includes(term)))
      .sort((a, b) => {
        const comparison = sort === 'time' ? (real ? a.timestamp! - b.timestamp! : a.event.launchedAt.localeCompare(b.event.launchedAt)) : a.film.title.localeCompare(b.film.title, 'zh-CN')
        return direction === 'asc' ? comparison : -comparison
      })
  }, [records, query, sort, direction, real])
  const dailyCounts = useMemo(() => {
    const counts = new Map<string, number>()
    for (const event of events) {
      const day = real ? localDateKey(eventDate(event.launchedAt)) : event.launchedAt.slice(0, 10)
      counts.set(day, (counts.get(day) ?? 0) + 1)
    }
    return counts
  }, [events, real])
  useEffect(() => { onActivityDays?.(loading || error ? null : activePlaybackDays(dailyCounts.keys())) }, [dailyCounts, loading, error, onActivityDays, active])
  const days = Array.from({ length: 7 }, (_, index) => dayKey(6 - index))
  const counts = days.map(day => dailyCounts.get(day) ?? 0)
  const maxCount = Math.max(1, ...counts)
  const recentCount = counts.reduce((total, count) => total + count, 0)
  const uniqueFilms = useMemo(() => new Set(records.map(({ film }) => film.id)).size, [records])
  const pageCount = Math.max(1, Math.ceil(visible.length / pageSize))
  const currentPage = Math.min(page, pageCount)
  const pageRows = visible.slice((currentPage - 1) * pageSize, currentPage * pageSize)

  const resetToFirstPage = () => { setPage(1) }
  const changePage = (nextPage: number) => { setPage(nextPage) }
  const clear = async () => {
    if (clearGuard.current) return
    if (!confirmClear) { setConfirmClear(true); return }
    clearGuard.current = true; setClearBusy(true); setClearError('')
    try { await onClear(); setConfirmClear(false); resetToFirstPage() }
    catch (reason) { setClearError(String(reason)) }
    finally { clearGuard.current = false; setClearBusy(false) }
  }
  const displayTime = (value: string) => real ? localLaunchTime(value) : value.replace('T', ' ')

  return <section className="history-page" aria-labelledby="history-title">
    <div className="history-heading"><div><h1 id="history-title">{t("播放历史")}</h1><p>{t("共 {0}次播放 · 涉及 {1}部影片", [records.length, uniqueFilms])}</p></div><div className="history-clear-actions">{confirmClear && <button type="button" disabled={clearBusy} onClick={() => setConfirmClear(false)}>{t("取消")}</button>}<button type="button" className={confirmClear ? 'confirm' : ''} disabled={events.length === 0 || clearBusy || loading || !!error} onClick={clear}>{confirmClear ? t("确认清除历史") : t("清除历史")}</button></div></div>
    {clearError && <p className="real-save-error" role="alert">{t(clearError)}</p>}

    <div className="history-chart" aria-label={t("近七天每日播放次数")}><div className="history-chart-heading"><div><span>LAST SEVEN DAYS</span><h2>{t("近七天播放次数")}</h2></div><div className="history-chart-total"><strong>{recentCount.toString().padStart(2, '0')}</strong><span>{t("次播放")}</span></div></div><div className="history-bars">{days.map((day, index) => <div className="history-day" key={day} role="img" aria-label={t("{0} 播放 {1} 次", [day, counts[index]])}><div className="history-bar-track"><span className="history-bar-count">{counts[index] || '—'}</span><i style={{ height: `${counts[index] / maxCount * 100}%` }} /></div><span className="history-day-label">{day.slice(5).replace('-', '/')}</span></div>)}</div></div>

    <div className="history-list-heading"><div><span className="section-kicker">PLAY RECORDS</span><h2>{t("播放记录")}</h2></div><div className="history-filters"><label className="history-search"><span aria-hidden="true">⌕</span><input type="search" value={query} onChange={event => { setQuery(event.target.value); resetToFirstPage() }} placeholder={t("搜索片名、编号或演员")} aria-label={t("搜索播放历史")} /></label><label className="history-sort">{t("排序")}<AppSelect value={sort} onChange={event => { setSort(event.target.value as 'time' | 'title'); resetToFirstPage() }} aria-label={t("历史排序依据")}><option value="time">{t("按播放时间")}</option><option value="title">{t("按影片名称")}</option></AppSelect></label><button type="button" className="direction-button" title={direction === 'asc' ? t("升序") : t("降序")} aria-label={t(direction === 'asc' ? "当前为升序，点击切换方向" : "当前为降序，点击切换方向")} onClick={() => { setDirection(value => value === 'asc' ? 'desc' : 'asc'); resetToFirstPage() }}><ArrowIcon direction={direction === 'asc' ? 'up' : 'down'} /></button></div></div>
    {error ? <div className="history-empty" role="alert"><p>{t(error)}</p>{onRetry && <button type="button" className="button" disabled={loading} onClick={onRetry}>{t("重新读取")}</button>}</div> : loading ? <div className="history-empty" role="status">{t("正在载入记录…")}</div> : records.length === 0 ? <div className="history-empty" role="status"><span aria-hidden="true">◷</span><h3>{t("暂无播放记录")}</h3><p>{t("播放记录会按时间显示在这里。")}</p></div> : visible.length === 0 ? <div className="history-empty" role="status"><h3>{t("没有符合条件的记录")}</h3><button type="button" onClick={() => { setQuery(''); resetToFirstPage() }}>{t("清除搜索")}</button></div> : <><div className="history-list">{pageRows.map(({ event, film }) => <button className="history-row" key={event.id} type="button" onClick={() => onOpenFilm(film.id)} aria-label={t("查看{0}详情，播放于{1}", [film.title, displayTime(event.launchedAt)])}><span className="history-row-cover"><FilmArtwork film={film} /></span><span className="history-row-film"><strong>{film.title}</strong><small>{film.codePlaceholder ? t(film.code) : film.code}{film.actor && <> <span>·</span> {film.actor}</>}</small><span className="history-film-extra">{film.library && <span className="history-film-library" title={film.library}>{film.library}</span>}<span className="history-film-rating">{film.rating === null ? t("未评分") : <>{t("评分")}<span className="rating-value">{film.rating.toFixed(1)}</span></>}</span></span></span><span className="history-row-time"><strong>{displayTime(event.launchedAt).slice(0, 10)}</strong><small>{displayTime(event.launchedAt).slice(11, 16)}</small></span><span className="history-row-arrow" aria-hidden="true">↗</span></button>)}</div>
      <ListPagination label={t("播放历史分页")} total={visible.length} pageSize={pageSize} page={currentPage} resetKey={JSON.stringify([query, sort, direction])} onChange={changePage} />
    </>}
  </section>
}
