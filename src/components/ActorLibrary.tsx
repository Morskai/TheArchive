import { t, message as uiMessage, useLocale, type UiText } from '../i18n/react'
import type { DisplayController } from './useDisplayPreferences'
import { RatioButton } from './RatioButton'
import { actorRatios, nextRatio, ratioLabel } from '../data/displayRatios'
import { AppSelect } from './AppSelect'
import { LibraryEmptyState } from './LibraryEmptyState'
import { actorAliases } from '../data/deliveryPresentation'
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { ArrowIcon, FilterChip } from './ui'
import { ScrollToTopButton } from './ScrollToTopButton'
import { actorSelectionScope, filterActors, selectedActorIds, type Actor } from '../data/actors'
import type { Film } from '../data/films'
import { actorFilms, averageActorRating, indexActorFilms } from '../data/actorMetrics'
import { useManualValue } from './useManualValue'
import { actorDeleteMessage, deleteActors, setActorFavorite } from '../data/realActors'
import { actorTagError, applyActorBulkTag, createActorTag, deleteActorTag, loadActorTags, renameActorTag, reorderActorTags } from '../data/actorTags'
import { tagFilmCounts } from '../data/tagPresentation'
import { TagFilters } from './TagFilters'
import { TagManager } from './TagManager'
import { realFilmsAvailable } from '../data/realFilms'
import { saveManagedActor } from '../data/actorManagement'
import { type ManagedActorInput } from '../data/actorManagementState'
import { localLaunchTime, eventDate } from '../data/localDates'
import { RealActorPortrait } from './RealActorPortrait'
import { FilmArtwork } from './FilmCard'
import { ActorEditor } from './ActorEditor'
import { ActorImport } from './ActorImport'
import { ActorExport } from './ActorExport'
import { ActorAdd } from './ActorAdd'
import { EditIcon } from './EditIcon'
import '../actor-library.css'

type ActorSort = 'name' | 'filmCount' | 'averageRating' | 'directoryCreatedAt' | 'directoryModifiedAt' | 'recentPlay'
type Density = 'comfortable' | 'standard' | 'compact'
const emptyActorFacts = { filmCount: 0, averageRating: null, recentPlay: null }

interface ActorLibraryProps {
  display?: DisplayController
  actors: Actor[]
  films: Film[]
  query: string
  missingPhotosOnly: boolean
  onClearPhotoFilter: () => void
  detailOpen: boolean
  initialDetailId: string | null
  onClearQuery: () => void
  onDetailViewChange: (open: boolean) => void
  onDetailBack: () => void
  onPortraitBack?: (close: () => void, animate: boolean) => void
  onUpdateActor: (id: string, patch: Partial<Actor>) => Promise<void>
  onOpenFilm: (id: string) => void
}

export function ActorPortrait({ actor, large = false }: { actor: Actor; large?: boolean }) {
  useLocale()
  if (actor.realActorId) return <RealActorPortrait actor={actor} large={large} />
  return <div className={`actor-portrait portrait-${actor.portrait} ${large ? 'actor-portrait-large' : ''}`}>
    <div className="portrait-light" aria-hidden="true" />
    <span className="portrait-photo" role="img" aria-label={t("{0}照片", [actor.name])} />
    {!large && <span className="portrait-hint" aria-hidden="true">{t("查看资料 ↗")}</span>}
  </div>
}

function ActorProfilePage({ actor, films, onUpdate, onBack, onOpenFilm }: { actor: Actor; films: Film[]; onUpdate: (id: string, patch: Partial<Actor>) => Promise<void>; onBack: (animate: boolean) => void; onOpenFilm: (id: string) => void }) {
  useLocale()
  const favorite = useManualValue('actor-favorite:' + actor.id, actor.favorite, value => actor.realActorId ? setActorFavorite(actor.realActorId, value) : onUpdate(actor.id, { favorite: value }))
  const backRef = useRef<HTMLButtonElement>(null)
  const [editing, setEditing] = useState(false)
  const [notice, setNotice] = useState<UiText>('')
  useEffect(() => { backRef.current?.focus({ preventScroll: true }) }, [])
  const linkedFilms = useMemo(() => actorFilms(actor.id, films), [actor.id, films])
  const relatedGrid = useRef<HTMLDivElement>(null)
  const [relatedHeight, setRelatedHeight] = useState<number>()
  useLayoutEffect(() => {
    const grid = relatedGrid.current
    if (!grid) return
    const measure = () => {
      const cards = [...grid.children] as HTMLElement[]
      const columns = getComputedStyle(grid).gridTemplateColumns.split(' ').length
      const last = cards[Math.min(cards.length, columns * 3) - 1]
      if (last) setRelatedHeight(last.offsetTop + last.offsetHeight + 4)
    }
    const observer = new ResizeObserver(measure)
    observer.observe(grid)
    for (const card of [...grid.children].slice(0, 12)) observer.observe(card)
    measure()
    return () => observer.disconnect()
  }, [linkedFilms])
  const averageRating = averageActorRating(linkedFilms)?.toFixed(1) ?? null
  const recentPlay = linkedFilms.flatMap(film => film.lastLaunchedAt ? [film.lastLaunchedAt] : []).sort((a, b) => eventDate(a).getTime() - eventDate(b).getTime()).at(-1)
  const favoriteFilms = linkedFilms.filter(film => film.favorite).length
  const measurements = [actor.bust, actor.waist, actor.hips]
  const profileValues = [actor.birthMonth, actor.height && t("身高 {0}cm", [actor.height]), actor.cup && t("罩杯 {0}", [actor.cup]), measurements.some(Boolean) && t("三围 {0}", [measurements.map(value => value || '—').join('/')])].filter((value): value is string => Boolean(value))
  const aliases = actor.aliases ?? actorAliases(actor.directoryName)
  const startEditing = () => { setNotice(''); setEditing(true) }
  const saveEditing = async (input: ManagedActorInput) => {
    if (actor.realActorId) await saveManagedActor(input)
    else await onUpdate(actor.id, { name: input.name, aliases: input.aliases.join('、'), ...input.profile, filmIds: input.filmIds })
    setEditing(false); setNotice('演员资料已保存。')
  }

  return <section className="actor-profile-page" aria-labelledby="actor-profile-title">
    <button className="actor-back" ref={backRef} type="button" onClick={event => onBack(event.detail > 0)}><ArrowIcon direction="left" /> {t("返回")}</button>
    <div className="actor-profile-layout">
      <aside className="actor-profile-left">
        <ActorPortrait actor={actor} large />
        <div className="actor-profile-identity"><div className="actor-profile-heading"><span className="section-kicker">{t("演员资料")}</span><button className="actor-edit-toggle" type="button" aria-label={t("编辑演员资料")} aria-haspopup="dialog" aria-expanded={editing} onClick={startEditing}><EditIcon /><span>{t("编辑资料")}</span></button></div><h1 id="actor-profile-title">{actor.name}</h1><p className="actor-linked-count">{t("参演影片 {0}部", [linkedFilms.length])}</p>{aliases && <p className="actor-directory-name">{t("别名：{0}", [aliases])}</p>}</div>
        {profileValues.length > 0 && <div className="actor-profile-values">{profileValues.map(value => <span key={value}>{value}</span>)}</div>}
        {notice && <p className="actor-management-notice" role="status">{t(notice)}</p>}
        <button type="button" aria-busy={favorite.saving} className={`actor-favorite ${favorite.value ? 'is-favorite' : ''}`} aria-pressed={favorite.value} onClick={() => favorite.set(!favorite.value)}>{favorite.value ? t("✦ 已收藏") : t("✧ 加入收藏")}</button>
        {favorite.error && <p className="actor-edit-error" role="alert">{t("收藏未保存，已恢复上次保存状态，请重试。")}</p>}
      </aside>
      <div className="actor-profile-main">
        <div className="actor-metrics" aria-label={t("演员参演资料")}>
          <div className="actor-metric-favorites"><strong>{favoriteFilms}</strong><span>{t("已收藏影片")}</span></div>
          <div><strong>{linkedFilms.length}</strong><span>{t("参演影片")}</span></div>
          <div><strong>{averageRating ?? '—'}</strong><span>{t("平均评分")}</span></div>
          <div><strong className="metric-date" title={recentPlay ? localLaunchTime(recentPlay) : undefined}>{recentPlay ? (actor.realActorId ? localLaunchTime(recentPlay) : recentPlay) : '—'}</strong><span>{t("最近播放时间")}</span></div>
        </div>
        <div className="actor-related-heading"><h2>{t("参演影片")}</h2><span>{t("{0}部", [linkedFilms.length])}</span></div>
        {linkedFilms.length === 0 ? <div className="actor-films-empty">{t("暂无参演影片")}</div> : <div className="actor-related-grid" ref={relatedGrid} style={{ maxHeight: relatedHeight }} tabIndex={0} aria-label={t("参演影片，可滚动查看更多")}>{linkedFilms.map(film => <button key={film.id} className="actor-related-card" type="button" onClick={() => onOpenFilm(film.id)} aria-label={t("查看{0}详情", [film.title])}><div className="actor-related-cover"><FilmArtwork film={film} /></div><strong>{film.title}</strong><span>{film.codePlaceholder ? t(film.code) : film.code}</span></button>)}</div>}
        <div className="actor-source-facts"><span>{t("关联片库：")}{actor.library || t("暂无关联")}</span><span>{t("创建时间：")}{actor.directoryCreatedAt ? actor.directoryCreatedAt.includes('T') ? localLaunchTime(actor.directoryCreatedAt) : actor.directoryCreatedAt : t("暂无资料")}</span></div>
      </div>
    </div>
    {editing && <ActorEditor actor={actor} films={films} onSave={saveEditing} onClose={() => setEditing(false)} />}
  </section>
}

export const ActorLibrary = memo(function ActorLibrary({ display, actors, films, query, missingPhotosOnly, onClearPhotoFilter, detailOpen, initialDetailId, onClearQuery, onDetailViewChange, onDetailBack, onPortraitBack, onUpdateActor, onOpenFilm }: ActorLibraryProps) {
  useLocale()
  const [favoritesOnly, setFavoritesOnly] = useState(false)
  const [tags, setTags] = useState<string[]>([])
  const [selectedTags, setSelectedTags] = useState<string[]>([])
  const [tagManagerOpen, setTagManagerOpen] = useState(false)
  const [selecting, setSelecting] = useState(false)
  const [selection, setSelection] = useState({ scope: '', ids: [] as string[] })
  const [bulkTag, setBulkTag] = useState('')
  const [busy, setBusy] = useState(false)
  const busyRef = useRef(false)
  const selectAllRef = useRef<HTMLInputElement>(null)
  const [saveError, setSaveError] = useState('')
  const [deletedIds, setDeletedIds] = useState<string[]>([])
  const [tagPatches, setTagPatches] = useState<Record<string, string[]>>({})
  const [sort, setSort] = useState<ActorSort>('name')
  const [direction, setDirection] = useState<'asc' | 'desc'>('asc')
  const [density, setDensity] = useState<Density>('standard')
  const [detailId, setDetailId] = useState<string | null>(initialDetailId)
  const [manager, setManager] = useState<'add' | 'new' | 'import' | 'export' | null>(null)
  const [managementNotice, setManagementNotice] = useState<UiText>('')
  const returnFocusId = useRef<string | null>(null)
  const closeTagManager = useCallback(() => setTagManagerOpen(false), [])
  useEffect(() => {
    if (!realFilmsAvailable) return
    let revision = 0
    let alive = true
    const refresh = () => {
      const current = ++revision
      void loadActorTags().then(names => {
        if (!alive || current !== revision) return
        setTags(names)
        setSelectedTags(selected => selected.filter(tag => names.includes(tag)))
      }).catch(() => { if (alive && current === revision) setSaveError('演员标签暂时无法读取，请重试。') })
    }
    refresh()
    window.addEventListener('real-library-changed', refresh)
    return () => { alive = false; revision++; window.removeEventListener('real-library-changed', refresh) }
  }, [])
  useEffect(() => setTagPatches({}), [actors])
  const currentActors = useMemo(() => actors.filter(actor => !deletedIds.includes(actor.id)).map(actor => Object.hasOwn(tagPatches, actor.id) ? { ...actor, tags: tagPatches[actor.id] } : actor), [actors, deletedIds, tagPatches])
  useLayoutEffect(() => {
    if (initialDetailId) { returnFocusId.current = null; setDetailId(initialDetailId) }
  }, [initialDetailId])
  useEffect(() => {
    if (detailOpen) return
    setDetailId(null)
    if (!returnFocusId.current) return
    const frame = requestAnimationFrame(() => document.querySelector<HTMLElement>(`[data-actor-id="${returnFocusId.current}"]`)?.focus({ preventScroll: true }))
    return () => cancelAnimationFrame(frame)
  }, [detailOpen])
  const detail = currentActors.find(actor => actor.id === detailId) ?? null
  const closeDetail = useCallback(() => { setDetailId(null); onDetailViewChange(false); onDetailBack() }, [onDetailViewChange, onDetailBack])
  useEffect(() => { if (detailOpen && detailId && !detail) closeDetail() }, [detailOpen, detailId, detail, closeDetail])
  const openDetail = (id: string) => { returnFocusId.current = id; setDetailId(id); onDetailViewChange(true) }
  const filmsByActor = useMemo(() => indexActorFilms(films), [films])
  const factsByActor = useMemo(() => new Map([...filmsByActor].map(([id, related]) => {
    const launches = related.flatMap(film => film.lastLaunchedAt === null ? [] : [eventDate(film.lastLaunchedAt).getTime()])
    return [id, { filmCount: related.length, averageRating: averageActorRating(related), recentPlay: launches.length ? Math.max(...launches) : null }]
  })), [filmsByActor])
  const actorFacts = useCallback((actor: Actor) => factsByActor.get(actor.id) ?? emptyActorFacts, [factsByActor])
  const visible = useMemo(() => {
    return filterActors(currentActors, { query, favoritesOnly, selectedTags, missingPhotosOnly }).sort((a, b) => {
      const left = sort === 'name' ? a.name : sort === 'directoryCreatedAt' ? a.directoryCreatedAt : sort === 'directoryModifiedAt' ? a.directoryModifiedAt ?? null : actorFacts(a)[sort]
      const right = sort === 'name' ? b.name : sort === 'directoryCreatedAt' ? b.directoryCreatedAt : sort === 'directoryModifiedAt' ? b.directoryModifiedAt ?? null : actorFacts(b)[sort]
      if (left === null) return right === null ? 0 : 1
      if (right === null) return -1
      const comparison = typeof left === 'number' && typeof right === 'number' ? left - right : String(left).localeCompare(String(right), 'zh-CN')
      return direction === 'asc' ? comparison : -comparison
    })
  }, [currentActors, query, favoritesOnly, selectedTags, missingPhotosOnly, sort, direction, actorFacts])
  const scope = actorSelectionScope({ query, favoritesOnly, selectedTags, missingPhotosOnly })
  const selectedIds = selectedActorIds(selection, scope, visible)
  const selectedCount = selectedIds.length
  const allSelected = visible.length > 0 && selectedCount === visible.length
  const partiallySelected = selectedCount > 0 && !allSelected
  const tagCounts = useMemo(() => tagFilmCounts(visible.map(actor => ({ tags: actor.tags ?? [] }))), [visible])
  const bulkTagValue = tags.includes(bulkTag) ? bulkTag : tags[0] ?? ''
  const activeFilterCount = Number(Boolean(query.trim())) + Number(favoritesOnly) + selectedTags.length + Number(missingPhotosOnly)
  useLayoutEffect(() => { if (selectAllRef.current) selectAllRef.current.indeterminate = partiallySelected }, [partiallySelected, selecting])
  // The scope check above makes old selections unusable before this cleanup runs.
  useEffect(() => { setSelection(current => current.scope === scope ? current : { scope, ids: [] }) }, [scope])
  const resetFilters = () => { onClearQuery(); onClearPhotoFilter(); setFavoritesOnly(false); setSelectedTags([]) }
  const toggleSelectionMode = () => { if (busyRef.current) return; setSelecting(value => !value); setSelection({ scope, ids: [] }); setManagementNotice(''); setSaveError('') }
  const toggleActor = (id: string) => {
    if (busyRef.current) return
    setSelection({ scope, ids: selectedIds.includes(id) ? selectedIds.filter(item => item !== id) : [...selectedIds, id] })
    setManagementNotice('')
  }
  const patchTags = (transform: (tags: string[]) => string[], ids?: Set<string>) => setTagPatches(current => {
    const next = { ...current }
    for (const actor of currentActors) if (!ids || ids.has(actor.id)) next[actor.id] = transform(actor.tags ?? [])
    return next
  })
  const addTag = async (name: string) => { await createActorTag(name); setTags(current => current.includes(name) ? current : [...current, name]) }
  const renameTag = async (name: string, newName: string) => {
    await renameActorTag(name, newName)
    setTags(current => current.map(tag => tag === name ? newName : tag))
    setSelectedTags(current => current.map(tag => tag === name ? newName : tag))
    setBulkTag(current => current === name ? newName : current)
    patchTags(names => names.map(tag => tag === name ? newName : tag))
  }
  const removeTag = async (name: string) => {
    if (!window.confirm(t("删除标签“{0}”会从所有已标记的演员中移除。确定删除吗？", [name]))) return
    try { await deleteActorTag(name) }
    catch (error) { setSaveError(actorTagError(error)); throw error }
    setTags(current => current.filter(tag => tag !== name)); setSelectedTags(current => current.filter(tag => tag !== name))
    patchTags(names => names.filter(tag => tag !== name))
  }
  const applyBulkTag = async (add: boolean) => {
    if (busyRef.current || !bulkTagValue || !selectedIds.length) return
    const ids = [...selectedIds]
    busyRef.current = true; setBusy(true); setSaveError(''); setManagementNotice('')
    try {
      await applyActorBulkTag(ids, bulkTagValue, add)
      patchTags(names => add ? [...new Set([...names, bulkTagValue])] : names.filter(tag => tag !== bulkTagValue), new Set(ids))
      setManagementNotice(uiMessage(add ? "已为 {0} 位演员添加标签“{1}”。" : "已从 {0} 位演员移除标签“{1}”。", [ids.length, bulkTagValue]))
      setSelection({ scope, ids: [] })
    } catch (error) { setSaveError(actorTagError(error)) }
    finally { busyRef.current = false; setBusy(false) }
  }
  const removeSelectedActors = async () => {
    if (busyRef.current || !selectedIds.length || !realFilmsAvailable) return
    const ids = [...selectedIds]
    if (!window.confirm(t("确定删除 {0} 位演员吗？\n\n将删除所选演员的资料、收藏、照片来源和全部出演关系。影片、影片标签、其他演员、演员标签及外部原文件会保留。", [ids.length]))) return
    busyRef.current = true; setBusy(true); setSaveError(''); setManagementNotice('')
    try {
      const count = await deleteActors(ids)
      setDeletedIds(current => [...new Set([...current, ...ids])]); setSelection({ scope, ids: [] })
      setManagementNotice(uiMessage("已删除 {0} 位演员。", [count]))
    } catch (error) { setSaveError(actorDeleteMessage(error)) }
    finally { busyRef.current = false; setBusy(false) }
  }
  const createActor = async (input: ManagedActorInput) => { await saveManagedActor({ ...input, actorId: undefined }); setManager(null); setManagementNotice('演员已创建，可打开详情继续编辑。'); resetFilters() }
  if (detail && detailOpen) return <ActorProfilePage key={detail.id} actor={detail} films={films} onUpdate={onUpdateActor} onBack={animate => onPortraitBack ? onPortraitBack(closeDetail, animate) : closeDetail()} onOpenFilm={onOpenFilm} />
  return <section className="actor-library library-section" aria-labelledby="actors-heading">
    <ScrollToTopButton />
    <div className="section-heading actor-management-heading"><div><h1 id="actors-heading">{t("演员库")}</h1><p className="collection-summary">{t("共 {0}位演员 · 标签 {1}个 · 加入收藏 {2}位", [currentActors.length, tags.length, currentActors.filter(actor => actor.favorite).length])}</p></div><div className="library-actions actor-management-toolbar" aria-label={t("演员资料管理")}><button type="button" disabled={!realFilmsAvailable || busy} onClick={() => setManager('add')}>{t("新增演员")}</button><button type="button" disabled={!realFilmsAvailable || currentActors.length === 0 || busy} onClick={() => setManager('export')}>{t("导出资料")}</button><button type="button" disabled={!realFilmsAvailable || busy} onClick={() => setTagManagerOpen(true)}>{t("标签编辑")}</button><button type="button" className={selecting ? 'active' : ''} disabled={currentActors.length === 0 || busy} aria-pressed={selecting} onClick={toggleSelectionMode}>{selecting ? t("退出演员管理") : t("演员管理")}</button></div></div>
    {managementNotice && <p className="actor-management-notice" role="status">{t(managementNotice)}</p>}
    {!realFilmsAvailable && <p className="actor-management-hint">{t("当前为演示资料。演员新建、导入与导出在 Windows 桌面版使用。")}</p>}
    <div className="controls"><div className="filter-row"><div className="control-label">{t("筛选")}</div><FilterChip active={favoritesOnly} onClick={() => setFavoritesOnly(value => !value)}>✦ <span>{t("已收藏")}</span></FilterChip><div className="filter-divider" /><TagFilters tags={tags} counts={tagCounts} showCounts={!selecting} selected={selectedTags} onToggle={tag => setSelectedTags(current => current.includes(tag) ? current.filter(item => item !== tag) : [...current, tag])} onReorder={async names => { await reorderActorTags(names); setTags(names) }} /></div>
      <div className="toolbar"><div className="result-count" aria-live="polite"><strong>{visible.length.toString().padStart(2, '0')}</strong><span>{t("位演员")}</span>{activeFilterCount > 0 && <button type="button" className="clear-link" onClick={resetFilters}>{t("清除筛选 · {0}", [activeFilterCount])}</button>}</div><div className="toolbar-actions"><label className="sort-label"><span>{t("排序")}</span><AppSelect value={sort} onChange={event => setSort(event.target.value as ActorSort)} aria-label={t("演员排序依据")}><option value="name">{t("按演员姓名")}</option><option value="filmCount">{t("按参演数量")}</option><option value="averageRating">{t("按平均评分")}</option><option value="directoryCreatedAt">{t("按创建时间")}</option><option value="directoryModifiedAt">{t("按修改时间")}</option><option value="recentPlay">{t("按最近播放时间")}</option></AppSelect></label><button className="direction-button" type="button" onClick={() => setDirection(value => value === 'asc' ? 'desc' : 'asc')} aria-label={t(direction === 'asc' ? "当前为升序，点击切换方向" : "当前为降序，点击切换方向")} title={direction === 'asc' ? t("升序") : t("降序")}><ArrowIcon direction={direction === 'asc' ? 'up' : 'down'} /></button><div className="toolbar-divider" /><RatioButton label={t("演员照片比例")} value={ratioLabel(display?.value.actorRatio ?? 'original', true)} next={ratioLabel(nextRatio(actorRatios, display?.value.actorRatio ?? 'original'), true)} disabled={display?.busy || !display?.loaded} onClick={() => void display?.setActor(nextRatio(actorRatios, display.value.actorRatio))} /><div className="toolbar-divider" /><fieldset className="density-control"><legend>{t("卡片密度")}</legend>{([['comfortable', t("舒适"), '▣'], ['standard', t("标准"), '▥'], ['compact', t("紧凑"), '▦']] as const).map(([value, label, icon]) => <button key={value} type="button" className={density === value ? 'active' : ''} aria-label={t("{0}密度", [label])} aria-pressed={density === value} title={t("{0}密度", [label])} onClick={() => setDensity(value)}>{icon}</button>)}</fieldset></div></div>
    </div>
    {selecting && <div className="actor-selection-bar" aria-busy={busy}>
      <div className="actor-selection-summary"><label className="actor-select-all"><input ref={selectAllRef} type="checkbox" aria-label={t("全选当前筛选结果")} aria-checked={partiallySelected ? 'mixed' : allSelected} checked={allSelected} disabled={busy || !visible.length} onChange={event => { setSelection({ scope, ids: event.target.checked ? [...new Set(visible.map(actor => actor.id))] : [] }); setManagementNotice('') }} /><span>{t("全选筛选结果")}</span></label><span className="actor-selection-count" aria-live="polite">{t("已选")}<strong>{selectedCount}</strong> {t("位")}<span>{t("／当前 {0}位", [visible.length])}</span></span><button type="button" disabled={busy || selectedCount === 0} onClick={() => setSelection({ scope, ids: [] })}>{t("清空选择")}</button></div>
      <div className="actor-selection-actions"><label>{t("目标标签")}<AppSelect value={bulkTagValue} onChange={event => setBulkTag(event.target.value)} disabled={busy || tags.length === 0} aria-label={t("演员批量操作目标标签")}>{tags.length === 0 && <option value="">{t("暂无标签")}</option>}{tags.map(tag => <option value={tag} key={tag}>{tag}</option>)}</AppSelect></label><button type="button" disabled={busy || !selectedCount || !bulkTagValue || !realFilmsAvailable} onClick={() => applyBulkTag(true)}>{t("添加标签")}</button><button type="button" disabled={busy || !selectedCount || !bulkTagValue || !realFilmsAvailable} onClick={() => applyBulkTag(false)}>{t("移除标签")}</button><button type="button" className="actor-delete-button" disabled={busy || !selectedCount || !realFilmsAvailable} onClick={removeSelectedActors}>{t("删除演员")}</button></div>
    </div>}
    {display?.error && <p className="actor-edit-error" role="alert">{t(display.error)}{!display.loaded && <button type="button" className="clear-link" onClick={display.retry}>{t("重试")}</button>}</p>}
    {saveError && <p className="actor-edit-error actor-library-error" role="alert">{saveError}</p>}
    {currentActors.length === 0 ? <LibraryEmptyState kind="actors" title={t("暂无演员资料")}><button className="button button-gold" type="button" disabled={!realFilmsAvailable} onClick={() => setManager('add')}>{t("新增演员")}</button></LibraryEmptyState> : visible.length === 0 ? <div className="empty-state" role="status"><div className="empty-orbit" aria-hidden="true">✦</div><h3>{missingPhotosOnly && !currentActors.some(actor => actor.hasPortrait === false) ? t("暂无照片缺失的演员") : t("没有符合条件的演员")}</h3><p>{t("试试更换关键词或筛选条件。")}</p><button className="button button-gold" type="button" onClick={resetFilters}>{t("清除筛选")}</button></div> : <div className={`actor-grid actor-density-${density} ${selecting ? 'actor-grid-selecting' : ''}`}>{visible.map(actor => {
      const facts = actorFacts(actor)
      const selected = selectedIds.includes(actor.id)
      return <button className={`actor-card ${selected ? 'actor-card-selected' : ''}`} type="button" key={actor.id} data-actor-id={actor.id} disabled={selecting && busy} onClick={() => selecting ? toggleActor(actor.id) : openDetail(actor.id)} aria-label={selecting ? t(selected ? "取消选择演员：{0}" : "选择演员：{0}", [actor.name]) : t("查看{0}详情", [actor.name])} aria-pressed={selecting ? selected : undefined}><ActorPortrait actor={actor} />{selecting && <span className="actor-selection-mark" aria-hidden="true"><svg viewBox="0 0 16 16"><path d="m3 8 3 3 7-7" /></svg></span>}<div className="actor-card-meta"><div><span className="actor-card-name">{actor.name}</span>{actor.favorite && <span className="actor-card-favorite" aria-label={t("已收藏")}>✦</span>}</div><p><span>{t("{0}部参演影片", [facts.filmCount])}</span><span className="actor-card-rating">{facts.averageRating === null ? t("暂无评分") : <><span className="star-shape filled" aria-hidden="true" />{facts.averageRating.toFixed(1)}</>}</span></p>{(actor.tags ?? []).length > 0 && <div className="actor-card-tags" aria-label={t("演员标签")}>{actor.tags!.map(tag => <span key={tag} title={tag}>{tag}</span>)}</div>}</div></button>
    })}</div>}
    {manager === 'add' && <ActorAdd onManual={() => setManager('new')} onScan={() => setManager('import')} onClose={() => setManager(null)} />}
    {manager === 'new' && <ActorEditor films={films} onSave={createActor} onClose={() => setManager(null)} />}
    {manager === 'import' && <ActorImport actors={actors} onClose={() => setManager(null)} />}
    {manager === 'export' && <ActorExport actors={currentActors} onClose={() => setManager(null)} />}
    {tagManagerOpen && <TagManager subject="演员" tags={tags} onAdd={addTag} onRename={renameTag} onDelete={removeTag} onClose={closeTagManager} />}
  </section>
})
