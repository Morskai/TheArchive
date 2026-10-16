import { t, message as uiMessage, useLocale, type UiText } from '../i18n/react'
import type { DisplayController } from './useDisplayPreferences'
import { RatioButton } from './RatioButton'
import { filmRatioForSources, filmRatios, nextRatio, ratioLabel, ratioCss } from '../data/displayRatios'
import { tagFilmCounts } from '../data/tagPresentation'
import { TagFilters } from './TagFilters'
import { LibraryEmptyState } from './LibraryEmptyState'
import { AppSelect } from './AppSelect'
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import { ArrowIcon, EmptyState, FilterChip } from './ui'
import { FilmCard } from './FilmCard'
import { ScrollToTopButton } from './ScrollToTopButton'
import { TagManager } from './TagManager'
import { RealFilmDetail, realFilmCardView } from './RealFilmDetail'
import { deleteRealFilms, renameRealTag, reorderRealTags, applyRealBulkTag, createRealTag, deleteRealTag, setRealFilmTags, type RealFilm } from '../data/realFilms'
import { selectedVisibleIds, visibleRealFilms, type RatingFilter, type RealSort } from '../data/realFilmFilters'
import '../real-film-library.css'
import { launchFilmVideo, notifyRealLibraryChanged, playbackError } from '../data/playback'
import { defaultFilmVideo, hasAmbiguousFilmCode, filmSourceNames } from '../data/realFilmPresentation'
import { updateArtworkAvailability } from '../data/artworkState'
import { invalidateChangedCovers } from '../data/covers'

type Density = 'comfortable' | 'standard' | 'compact'

interface RealFilmLibraryProps {
  display?: DisplayController
  films: RealFilm[]
  tags: string[]
  loading: boolean
  error: string
  onFilmsChange: Dispatch<SetStateAction<RealFilm[]>>
  onTagsChange: Dispatch<SetStateAction<string[]>>
  query: string
  onClearQuery: () => void
  density: Density
  onDensityChange: (value: Density) => void
  active: boolean
  onOpenActor: (id: string) => void
  onOpenScanSettings: () => void
}

function loadErrorMessage(error: unknown) {
  if (String(error) === 'protection_failed') return '建立应用资料保护备份失败，尚未处理旧观察。请检查应用数据目录后重试。'
  return '影片暂时无法载入。请稍后重试；现有文件与人工资料未被清除。'
}


export const RealFilmLibrary = memo(function RealFilmLibrary({ display, films, tags, loading, error: catalogError, onFilmsChange: setFilms, onTagsChange: setTags, query, onClearQuery, density, onDensityChange, active, onOpenActor, onOpenScanSettings }: RealFilmLibraryProps) {
  useLocale()
  const error = catalogError ? loadErrorMessage(catalogError) : ''
  const [saveError, setSaveError] = useState('')
  const [source, setSource] = useState('all')
  const [ratingFilter, setRatingFilter] = useState<RatingFilter>('all')
  const [favoritesOnly, setFavoritesOnly] = useState(false)
  const [selectedTags, setSelectedTags] = useState<string[]>([])
  const [sort, setSort] = useState<RealSort>('title')
  const [direction, setDirection] = useState<'asc' | 'desc'>('asc')
  const [detailId, setDetailId] = useState<string | null>(null)
  const [tagManagerOpen, setTagManagerOpen] = useState(false)
  const [tagEditing, setTagEditing] = useState(false)
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [bulkTag, setBulkTag] = useState('')
  const [bulkNotice, setBulkNotice] = useState<UiText>('')
  const [bulkBusy, setBulkBusy] = useState(false)
  const bulkGuard = useRef(false)
  const closeDetail = useCallback(() => setDetailId(null), [])
  const closeTagManager = useCallback(() => setTagManagerOpen(false), [])
  useLayoutEffect(() => {
    if (!active) { setDetailId(null); setTagManagerOpen(false) }
  }, [active])
  useEffect(() => {
    const coverStatus = (event: Event) => {
      const { filmId, available } = (event as CustomEvent<{ filmId: string; available: boolean }>).detail
      setFilms(current => updateArtworkAvailability(current, filmId, 'has_cover', available))
    }
    window.addEventListener('real-cover-status', coverStatus)
    return () => window.removeEventListener('real-cover-status', coverStatus)
  }, [setFilms])

  const sources = useMemo(() => {
    const found = new Map<string, string>()
    films.forEach(film => film.videos.forEach(video => found.set(video.source_id, video.source_name)))
    return [...found].sort((a, b) => a[1].localeCompare(b[1], 'zh-CN'))
  }, [films])
  const preferences = display?.value ?? { defaultFilmRatio: 'original' as const, actorRatio: 'original' as const, libraryRatios: {} }
  const shownRatio = filmRatioForSources([], source, preferences, display?.overviewRatio ?? 'original')
  const followingRatio = nextRatio(filmRatios, shownRatio)
  const visible = useMemo(() => visibleRealFilms(films, { query, source, rating: ratingFilter, favoritesOnly, selectedTags, sort, direction }), [films, query, source, ratingFilter, favoritesOnly, selectedTags, sort, direction])
  const tagCounts = useMemo(() => tagFilmCounts(visible), [visible])
  const visibleIds = useMemo(() => new Set(visible.map(film => film.id)), [visible])
  useEffect(() => {
    setSelectedIds(current => {
      const next = selectedVisibleIds(current, visible)
      return next.length === current.length ? current : next
    })
  }, [visibleIds, visible])
  const selectedCount = selectedVisibleIds(selectedIds, visible).length
  const bulkTagValue = tags.includes(bulkTag) ? bulkTag : (tags[0] ?? '')
  const detail = films.find(film => film.id === detailId) ?? null
  const activeFilterCount = Number(query.trim().length > 0) + Number(source !== 'all') + Number(ratingFilter !== 'all') + Number(favoritesOnly) + selectedTags.length
  const resetFilters = () => { onClearQuery(); setRatingFilter('all'); setFavoritesOnly(false); setSelectedTags([]) }
  const toggleTagFilter = (tag: string) => setSelectedTags(current => current.includes(tag) ? current.filter(item => item !== tag) : [...current, tag])
  const toggleTagEditing = () => { setTagEditing(current => !current); setSelectedIds([]); setBulkNotice(''); setDetailId(null) }
  const toggleFilmSelection = (id: string) => { setSelectedIds(current => current.includes(id) ? current.filter(item => item !== id) : [...current, id]); setBulkNotice('') }

  const playFilm = async (film: RealFilm) => {
    const videoId = defaultFilmVideo(film)
    if (!videoId) return
    setDetailId(null); setSaveError('')
    try {
      const outcome = await launchFilmVideo(film.id, videoId)
      if (outcome.historySaved) notifyRealLibraryChanged()
      else setSaveError('播放器启动请求已成功，但历史保存失败。软件不会自动再次启动。')
    } catch (reason) { setSaveError(playbackError(reason)) }
  }

  const updateTags = async (id: string, names: string[]) => {
    await setRealFilmTags(id, names)
    notifyRealLibraryChanged()
    setFilms(current => current.map(film => film.id === id ? { ...film, tags: names } : film))
  }
  const addTag = async (name: string) => { await createRealTag(name); setTags(current => [...current, name]); notifyRealLibraryChanged() }
  const renameTag = async (name: string, newName: string) => {
    await renameRealTag(name, newName)
    setTags(current => current.map(tag => tag === name ? newName : tag))
    setSelectedTags(current => current.map(tag => tag === name ? newName : tag))
    setBulkTag(current => current === name ? newName : current)
    setFilms(current => current.map(film => ({ ...film, tags: film.tags.map(tag => tag === name ? newName : tag) })))
    notifyRealLibraryChanged()
  }
  const removeTag = async (name: string) => {
    if (!window.confirm(t("删除标签“{0}”会从所有已标记的影片中移除。确定删除吗？", [name]))) return
    await deleteRealTag(name)
    notifyRealLibraryChanged()
    setTags(current => current.filter(tag => tag !== name))
    setSelectedTags(current => current.filter(tag => tag !== name))
    setFilms(current => current.map(film => ({ ...film, tags: film.tags.filter(tag => tag !== name) })))
  }
  const applyBulkTag = async (add: boolean) => {
    if (!bulkTagValue || selectedCount === 0 || bulkGuard.current) return
    const ids = selectedVisibleIds(selectedIds, visible)
    bulkGuard.current = true; setBulkBusy(true)
    setSaveError('')
    try {
      await applyRealBulkTag(ids, bulkTagValue, add)
      notifyRealLibraryChanged()
      const affected = new Set(ids)
      setFilms(current => current.map(film => !affected.has(film.id) ? film : {
        ...film, tags: add ? (film.tags.includes(bulkTagValue) ? film.tags : [...film.tags, bulkTagValue]) : film.tags.filter(tag => tag !== bulkTagValue),
      }))
      setBulkNotice(uiMessage(add ? "已为 {0} 部影片添加标签“{1}”。" : "已从 {0} 部影片移除标签“{1}”。", [affected.size, bulkTagValue]))
      setSelectedIds([])
    } catch { setSaveError('批量标签未保存，所选影片保持不变。') }
    finally { bulkGuard.current = false; setBulkBusy(false) }
  }

  const deleteSelectedFilms = async () => {
    const ids = selectedVisibleIds(selectedIds, visible)
    if (!ids.length || bulkGuard.current) return
    if (!window.confirm(t("确定删除 {0} 部影片吗？\n\n将删除所选影片的软件资料及出演、标签关联和播放记录，保留磁盘影片文件。重新扫描原目录可再次收录。", [ids.length]))) return
    bulkGuard.current = true; setBulkBusy(true); setSaveError(''); setBulkNotice('')
    try {
      const count = await deleteRealFilms(ids)
      invalidateChangedCovers(ids)
      const removed = new Set(ids)
      setFilms(current => current.filter(film => !removed.has(film.id)))
      setSelectedIds([]); setDetailId(null)
      setBulkNotice(uiMessage("已删除 {0} 部影片。", [count]))
      notifyRealLibraryChanged()
    } catch (reason) {
      setSaveError(String(reason) === 'protection_failed' ? '资料保护失败，影片尚未删除，请稍后重试。' : '影片未能删除，原有资料已保留，请稍后重试。')
    } finally { bulkGuard.current = false; setBulkBusy(false) }
  }

  return <div className="page-content"><section className="library-section" aria-labelledby="library-heading">
    <ScrollToTopButton />
    <div className="section-heading"><div><h1 id="library-heading">{t("影片库")}</h1><p className="collection-summary">{t("共 {0}部影片 · 标签 {1}个 · 收藏 {2}部", [films.length, tags.length, films.filter(film => film.favorite).length])}</p></div><div className="library-actions"><button type="button" onClick={() => setTagManagerOpen(true)}>{t("标签编辑")}</button><button className={tagEditing ? 'active' : ''} type="button" aria-pressed={tagEditing} onClick={toggleTagEditing}>{tagEditing ? t("退出影片管理") : t("影片管理")}</button></div></div>
    <div className="controls"><div className="filter-row"><div className="control-label">{t("筛选")}</div>
      <label className="select-wrap"><span>{t("片库")}</span><AppSelect popupSizing="content" value={source} onChange={event => setSource(event.target.value)} aria-label={t("片库")}><option value="all">{t("全部片库")}</option>{sources.map(([id, name]) => <option key={id} value={id}>{name}</option>)}</AppSelect></label>
      <label className="select-wrap"><span>{t("评分")}</span><AppSelect popupSizing="content" value={ratingFilter} onChange={event => setRatingFilter(event.target.value as RatingFilter)} aria-label={t("评分")}><option value="all">{t("不限")}</option>{[5, 4, 3, 2, 1].map(stars => <option key={stars} value={String(stars)}>{t("{0}星", [stars])}</option>)}</AppSelect></label>
      <FilterChip active={favoritesOnly} onClick={() => setFavoritesOnly(value => !value)}>✦ <span>{t("已收藏")}</span></FilterChip><div className="filter-divider" /><TagFilters showCounts={!tagEditing} counts={tagCounts} tags={tags} selected={selectedTags} onToggle={toggleTagFilter} onReorder={async names => { await reorderRealTags(names); setTags(names); notifyRealLibraryChanged() }} />
    </div>
      <div className="toolbar"><div className="result-count" aria-live="polite"><strong>{visible.length.toString().padStart(2, '0')}</strong><span>{t("部影片")}</span>{activeFilterCount > 0 && <button type="button" className="clear-link" onClick={resetFilters}>{t("清除筛选 · {0}", [activeFilterCount])}</button>}</div><div className="toolbar-actions"><label className="sort-label"><span>{t("排序")}</span><AppSelect value={sort} onChange={event => setSort(event.target.value as RealSort)} aria-label={t("排序依据")}><option value="code">{t("按影片编号")}</option><option value="title">{t("按影片名称")}</option><option value="rating">{t("按个人评分")}</option><option value="source">{t("按来源片库")}</option><option value="createdAt">{t("按创建时间")}</option><option value="modifiedAt">{t("按修改时间")}</option><option value="lastLaunchedAt">{t("按最近播放时间")}</option></AppSelect></label><button className="direction-button" type="button" onClick={() => setDirection(value => value === 'asc' ? 'desc' : 'asc')} aria-label={t(direction === 'asc' ? "当前为升序，点击切换方向" : "当前为降序，点击切换方向")} title={direction === 'asc' ? t("升序") : t("降序")}><ArrowIcon direction={direction === 'asc' ? 'up' : 'down'} /></button><div className="toolbar-divider" /><RatioButton label={t("影片封面比例")} value={ratioLabel(shownRatio)} next={ratioLabel(followingRatio)} disabled={display?.busy || !display?.loaded} onClick={() => source === 'all' ? display?.setOverview(followingRatio) : void display?.setFilm(source, followingRatio)} /><div className="toolbar-divider" /><fieldset className="density-control"><legend>{t("卡片密度")}</legend>{([['comfortable', t("舒适"), '▣'], ['standard', t("标准"), '▥'], ['compact', t("紧凑"), '▦']] as const).map(([value, label, icon]) => <button key={value} type="button" className={density === value ? 'active' : ''} aria-label={t("{0}密度", [label])} aria-pressed={density === value} title={t("{0}密度", [label])} onClick={() => onDensityChange(value)}>{icon}</button>)}</fieldset></div></div>
    </div>
    {tagEditing && <div className="bulk-bar"><div className="bulk-count"><span className="bulk-check" aria-hidden="true">✓</span>{t("已选")}<strong>{selectedCount}</strong> {t("部影片")}<span className="bulk-visible">{t("／当前显示 {0}部影片", [visible.length])}</span></div><div className="bulk-actions"><label>{t("目标标签")}<AppSelect value={bulkTagValue} onChange={event => setBulkTag(event.target.value)} disabled={tags.length === 0 || bulkBusy} aria-label={t("批量操作目标标签")}>{tags.length === 0 && <option value="">{t("暂无标签")}</option>}{tags.map(tag => <option key={tag} value={tag}>{tag}</option>)}</AppSelect></label><button type="button" disabled={!selectedCount || !bulkTagValue || bulkBusy} onClick={() => applyBulkTag(true)}>{t("添加标签")}</button><button type="button" disabled={!selectedCount || !bulkTagValue || bulkBusy} onClick={() => applyBulkTag(false)}>{t("移除标签")}</button><button type="button" disabled={!selectedCount || bulkBusy} onClick={() => void deleteSelectedFilms()}>{t("删除影片")}</button></div>{bulkNotice && <span className="bulk-notice" role="status">{t(bulkNotice)}</span>}</div>}
    {display?.error && <p className="real-save-error" role="alert">{t(display.error)}{!display.loaded && <button type="button" className="clear-link" onClick={display.retry}>{t("重试")}</button>}</p>}
    {saveError && <p className="real-save-error" role="alert">{t(saveError)}</p>}
    {loading ? <div className="loading-state" role="status">{t("正在载入已记录的视频…")}</div>
      : error ? <div className="empty-state" role="alert"><h3>{t("暂时无法打开影片库")}</h3><p>{t(error)}</p></div>
      : films.length === 0 ? <LibraryEmptyState kind="films" title={t("尚未收录影片")} onMarkClick={onOpenScanSettings} />
      : visible.length === 0 ? <EmptyState onReset={resetFilters} title={t("没有符合条件的影片")} />
      : <div className={`film-grid density-${density}`}>{visible.map(film => <FilmCard coverRatio={ratioCss(filmRatioForSources(film.videos.map(video => video.source_id), source, preferences, display?.overviewRatio ?? 'original'))} key={film.id} film={realFilmCardView(film)} codePlaceholder={hasAmbiguousFilmCode(film)} sourceName={filmSourceNames(film)} editing={tagEditing} selected={selectedIds.includes(film.id)} onOpen={() => { if (active) { if (tagEditing) toggleFilmSelection(film.id); else setDetailId(film.id) } }} onPlay={() => { if (active && !tagEditing) void playFilm(film) }} />)}</div>}
  </section><RealFilmDetail film={active ? detail : null} availableTags={tags} onTags={updateTags} onClose={closeDetail} onOpenActor={onOpenActor} />
    {tagManagerOpen && <TagManager onRename={renameTag} tags={tags} onAdd={addTag} onDelete={removeTag} onClose={closeTagManager} />}
  </div>
})
