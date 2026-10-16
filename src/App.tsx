import { t, message as uiMessage, useLocale, type UiText } from './i18n/react'
import { ConfigurationDirectory } from './components/ConfigurationDirectory'
import { listen } from '@tauri-apps/api/event'
import { EmbeddedPlayerHost } from './components/EmbeddedPlayer'
import { clearOverviewRatio, useDisplayPreferences } from './components/useDisplayPreferences'
import { RatioButton } from './components/RatioButton'
import { filmRatios, nextRatio, ratioLabel, ratioCss, filmRatioForSources } from './data/displayRatios'
import { WindowCloseDialog } from './components/WindowCloseDialog'
import { tagFilmCounts } from './data/tagPresentation'
import { TextEditMenu } from './components/TextEditMenu'
import { TagFilters } from './components/TagFilters'
import { actorPhotoSummary } from './data/deliveryPresentation'
import { AppSelect } from './components/AppSelect'
import { listenFilmManualSaved, listenActorFavoriteSaved } from './data/manualEvents'
import { listenFavoriteSaved } from './data/filmFavoriteEvents'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import { createActorPortraitTransition } from './data/actorPortraitTransition'
import { ArrowIcon, Navigation, PageFrame, FilterChip, EmptyState, type Page } from './components/ui'
import { WindowTitleBar } from './components/WindowTitleBar'
import { FilmCard } from './components/FilmCard'
import { ScrollToTopButton } from './components/ScrollToTopButton'
import { FilmDetail } from './components/FilmDetail'
import { TagManager } from './components/TagManager'
import { ActorLibrary } from './components/ActorLibrary'
import { ActorFlip, getDrawPool } from './components/ActorFlip'
import { CometTrail } from './components/CometTrail'
import { PlaybackHistory } from './components/PlaybackHistory'
import { RealPlaybackHistory } from './components/RealPlaybackHistory'
import { ProblemCenter } from './components/ProblemCenter'
import { SettingsPage } from './components/SettingsPage'
import { RealFilmLibrary } from './components/RealFilmLibrary'
import { RealFilmDetail } from './components/RealFilmDetail'
import { usePageScroll } from './components/usePageScroll'
import { MotionPage } from './components/MotionPage'
import { useButtonPress, useMotionEnabled } from './components/useAppMotion'
import { invalidateCovers } from './data/covers'
import { realFilmsAvailable, setRealFilmTags, type RealFilm } from './data/realFilms'
import { loadActorCatalog, actorFilmView, saveActorProfile, setActorFavorite } from './data/realActors'
import { loadApplicationDataDirectory, notifyRealLibraryChanged } from './data/playback'
import { displayLibraryPath } from './data/libraryPath'
import { demoActors, type Actor } from './data/actors'
import { demoFilmSource, type Film } from './data/films'
import { demoHistorySource, filmsWithLaunchHistory, type LaunchEvent } from './data/history'
import { listLibrarySources } from './data/sources'
import { initialSettings, loadInterfacePreferences, type PreviewSettings } from './data/settings'
import { initializeLibrarySession } from './data/maintenance'
import type { RatingFilter } from './data/realFilmFilters'
import './styles.css'
import './film-library.css'
import './app-motion.css'
import './brand.css'

type SortField = 'code' | 'createdAt' | 'modifiedAt' | 'rating' | 'title' | 'lastLaunchedAt'
type Density = 'comfortable' | 'standard' | 'compact'
const initialTags = ['中文字幕', '剧情', '旅行', '夜景']

function compareFilms(a: Film, b: Film, field: SortField, direction: 'asc' | 'desc') {
  const left = field === 'code' ? a.code || null : a[field] ?? null, right = field === 'code' ? b.code || null : b[field] ?? null
  if (left === null) return right === null ? 0 : 1
  if (right === null) return -1
  const comparison = typeof left === 'number' && typeof right === 'number' ? left - right : String(left).localeCompare(String(right), 'zh-CN', { numeric: field === 'code' })
  return direction === 'asc' ? comparison : -comparison
}

function App() {
  const locale = useLocale()
  const portraitTravel = useRef(createActorPortraitTransition())
  const portraitMotion = useRef(false)
  const flipReturnFocus = useRef<string | null>(null)
  useEffect(() => () => portraitTravel.current.cancel(), [])
  const appElement = useRef<HTMLDivElement>(null)
  const [navigationCollapsed, setNavigationCollapsed] = useState(() => window.matchMedia('(max-width:1120px)').matches)
  const [activePage, setActivePage] = useState<Page>(() => sessionStorage.getItem('film-library-restore-complete') === '1' ? 'settings' : 'films')
  const [actorDetailOpen, setActorDetailOpen] = useState(false)
  const [actorEntryId, setActorEntryId] = useState<string | null>(null)
  const [actorEntryOrigin, setActorEntryOrigin] = useState<Page>('actors')
  const [scanSettingsRequested, setScanSettingsRequested] = useState(false)
  const [scanGuideActive, setScanGuideActive] = useState(false)
  const rememberPage = usePageScroll(activePage === 'actors' && actorDetailOpen ? `actor-detail:${actorEntryId ?? 'library'}` : activePage)
  useEffect(() => {
    if (!scanSettingsRequested || activePage !== 'settings') return
    const frame = requestAnimationFrame(() => {
      const heading = document.getElementById('settings-libraries-title')
      document.getElementById('settings-library-scan')?.scrollIntoView({ block: 'start', behavior: 'instant' })
      heading?.focus({ preventScroll: true })
      setScanGuideActive(true)
      setScanSettingsRequested(false)
    })
    return () => cancelAnimationFrame(frame)
  }, [scanSettingsRequested, activePage])
  useEffect(() => {
    if (!scanGuideActive || activePage !== 'settings') return
    const dismiss = () => setScanGuideActive(false)
    const keydown = (event: KeyboardEvent) => { if (event.key === 'Escape') dismiss() }
    window.addEventListener('click', dismiss, true)
    window.addEventListener('keydown', keydown)
    return () => { window.removeEventListener('click', dismiss, true); window.removeEventListener('keydown', keydown) }
  }, [scanGuideActive, activePage])
  const navigate = (page: Page) => {
    portraitTravel.current.cancel()
    setScanGuideActive(false)
    setScanSettingsRequested(false)
    rememberPage(); setActivePage(page); setDetailId(null); setTagManagerOpen(false)
    if (actorEntryOrigin !== 'actors') { setActorDetailOpen(false); setActorEntryId(null); setActorEntryOrigin('actors') }
  }
  useEffect(() => {
    if (!realFilmsAvailable) return
    let active = true
    let stop: (() => void) | undefined
    void listen('tray-open-settings', () => {
      if (active) { setScanGuideActive(false); setScanSettingsRequested(false); setActivePage('settings'); setActorDetailOpen(false); setDetailId(null); setTagManagerOpen(false) }
    }).then(unlisten => { if (active) stop = unlisten; else unlisten() })
    return () => { active = false; stop?.() }
  }, [])
  const openActor = useCallback((id: string, origin: Page, animate = false) => {
    const update = () => { rememberPage(); setDetailId(null); setActorEntryId(id); setActorEntryOrigin(origin); setActorDetailOpen(true); setActivePage('actors') }
    if (origin === 'actorFlip' && animate && portraitMotion.current) {
      portraitTravel.current.start(`.flip-card[data-actor-id="${CSS.escape(id)}"] .actor-portrait`, '.actor-profile-left > .actor-portrait', () => flushSync(update))
    } else { portraitTravel.current.cancel(); update() }
  }, [rememberPage])
  const openFilmActor = useCallback((id: string) => openActor(id, 'films'), [openActor])
  const clearFilmQuery = useCallback(() => setQuery(''), [])
  const clearActorQuery = useCallback(() => setActorQuery(''), [])
  const clearActorPhotoFilter = useCallback(() => setMissingActorPhotosOnly(false), [])
  const changeActorDetail = useCallback((open: boolean) => { rememberPage(); setActorDetailOpen(open) }, [rememberPage])
  const backFromActor = useCallback(() => {
    if (actorEntryOrigin === 'actorFlip') flipReturnFocus.current = actorEntryId
    if (actorEntryOrigin !== 'actors') setActivePage(actorEntryOrigin)
    setActorEntryId(null); setActorEntryOrigin('actors')
  }, [actorEntryOrigin, actorEntryId])
  useEffect(() => {
    if (activePage !== 'actorFlip' || !flipReturnFocus.current) return
    document.querySelector<HTMLButtonElement>(`.flip-card[data-actor-id="${CSS.escape(flipReturnFocus.current)}"]`)?.focus({ preventScroll: true })
    flipReturnFocus.current = null
  }, [activePage])
  const [actors, setActors] = useState<Actor[]>(realFilmsAvailable ? [] : demoActors)
  const [realActorFilms, setRealActorFilms] = useState<RealFilm[]>([])
  const [catalogLoadError, setCatalogLoadError] = useState('')
  const actorLoadError = catalogLoadError ? catalogLoadError === 'protection_failed' ? t("资料保护失败，演员资料暂未更新。") : t("演员资料暂时无法读取，请重试。") : ''
  const [films, setFilms] = useState<Film[]>([])
  const [enabledSourceCount, setEnabledSourceCount] = useState<number | null>(null)
  const [configurationDirectory, setConfigurationDirectory] = useState('正在读取…')
  const [historyActiveDays, setHistoryActiveDays] = useState<number | null>(null)
  const photoSummary = actorPhotoSummary(actors)
  const [historyEvents, setHistoryEvents] = useState<LaunchEvent[]>([])
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState(false)
  const [query, setQuery] = useState('')
  const [actorQuery, setActorQuery] = useState('')
  const [missingActorPhotosOnly, setMissingActorPhotosOnly] = useState(false)
  const [library, setLibrary] = useState('all')
  const [ratingFilter, setRatingFilter] = useState<RatingFilter>('all')
  const [favoritesOnly, setFavoritesOnly] = useState(false)
  const [tags, setTags] = useState(realFilmsAvailable ? [] : initialTags)
  const [selectedTags, setSelectedTags] = useState<string[]>([])
  const [sort, setSort] = useState<SortField>('createdAt')
  const [direction, setDirection] = useState<'asc' | 'desc'>('desc')
  const [density, setDensity] = useState<Density>('standard')
  const [detailId, setDetailId] = useState<string | null>(null)
  const [tagManagerOpen, setTagManagerOpen] = useState(false)
  const [tagEditing, setTagEditing] = useState(false)
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [bulkTag, setBulkTag] = useState(initialTags[0])
  const [bulkNotice, setBulkNotice] = useState<UiText>('')
  const [settings, setSettings] = useState<PreviewSettings>(initialSettings)
  const motionEnabled = useMotionEnabled(settings.reduceMotion)
  portraitMotion.current = motionEnabled
  const backFromPortrait = useCallback((close: () => void, animate: boolean) => {
    if (actorEntryOrigin === 'actorFlip' && actorEntryId && animate && motionEnabled) {
      portraitTravel.current.start('.actor-profile-left > .actor-portrait', `.flip-card[data-actor-id="${CSS.escape(actorEntryId)}"] .actor-portrait`, () => flushSync(close))
    } else { portraitTravel.current.cancel(); close() }
  }, [actorEntryOrigin, actorEntryId, motionEnabled])
  useEffect(() => { if (!motionEnabled) portraitTravel.current.cancel(true) }, [motionEnabled])
  useButtonPress(appElement, motionEnabled)
  const preferencesRevision = useRef(0)
  const [sessionReady, setSessionReady] = useState(!realFilmsAvailable)
  const [startupBusy, setStartupBusy] = useState(realFilmsAvailable)
  const [startupMessage, setStartupMessage] = useState('')
  const [startupError, setStartupError] = useState(false)
  const [dataRevision, setDataRevision] = useState(0)
  const displayPreferences = useDisplayPreferences(sessionReady, dataRevision)
  const flipEntranceSession = useRef({ played: false })

  const prepareSession = useCallback(async () => {
    if (!realFilmsAvailable) return
    setStartupBusy(true); setStartupError(false)
    try { await initializeLibrarySession(); setStartupMessage('') }
    catch { setStartupError(true); setStartupMessage('启动资料暂时无法读取，请重试读取资料。') }
    finally { setStartupBusy(false); setSessionReady(true) }
  }, [])
  useEffect(() => { void prepareSession() }, [prepareSession])

  useEffect(() => {
    if (!realFilmsAvailable || !sessionReady) return
    let active = true
    const revision = preferencesRevision.current
    loadInterfacePreferences().then(preferences => {
      if (active && revision === preferencesRevision.current) setSettings(current => ({ ...current, ...preferences }))
    }).catch(() => {})
    return () => { active = false }
  }, [sessionReady])

  useEffect(() => {
    if (!sessionReady) return
    if (realFilmsAvailable) loadApplicationDataDirectory().then(path => setConfigurationDirectory(displayLibraryPath(path))).catch(() => setConfigurationDirectory('暂时无法读取'))
    if (realFilmsAvailable) {
      let active = true, revision = 0
      const refresh = () => {
        const request = ++revision
        void loadActorCatalog().then(snapshot => {
          if (!active || request !== revision) return
          setActors(snapshot.actors.map(actor => ({ ...actor, realActorId: actor.id, portrait: 0 })))
          setRealActorFilms(snapshot.films); setFilms(snapshot.films.map(actorFilmView)); setTags(snapshot.tags)
          setCatalogLoadError('')
        }).catch(error => { if (active && request === revision) setCatalogLoadError(String(error)) })
          .finally(() => { if (active && request === revision) setLoading(false) })
      }
      refresh()
      let sourceCountRevision = 0
      const refreshSourceCount = () => { const requested = ++sourceCountRevision; void listLibrarySources().then(sources => { if (active && requested === sourceCountRevision) setEnabledSourceCount(sources.filter(source => source.enabled && !source.removed).length) }).catch(() => { if (active && requested === sourceCountRevision) setEnabledSourceCount(null) }) }
      refreshSourceCount()
      window.addEventListener('real-library-changed', refreshSourceCount)
      window.addEventListener('real-library-changed', refresh)
      window.addEventListener('focus', refresh)
      return () => { active = false; window.removeEventListener('real-library-changed', refreshSourceCount); window.removeEventListener('real-library-changed', refresh); window.removeEventListener('focus', refresh) }
    }
    Promise.all([demoFilmSource.listFilms(), demoHistorySource.listLaunchEvents()])
      .then(([filmRows, events]) => { setHistoryEvents(events); setFilms(filmsWithLaunchHistory(filmRows, events)) })
      .catch(() => setLoadError(true)).finally(() => setLoading(false))
  }, [sessionReady])

  useEffect(() => listenFilmManualSaved((id, patch) => {
    setRealActorFilms(current => current.map(film => film.id === id ? { ...film, ...patch } : film))
    setFilms(current => current.map(film => film.id === id ? { ...film, ...(patch.rating !== undefined ? { rating: patch.rating } : {}), ...(patch.preferred_video_id !== undefined ? { preferredVersionId: patch.preferred_video_id } : {}), ...(patch.tags !== undefined ? { tags: patch.tags } : {}) } : film))
  }), [])
  useEffect(() => listenActorFavoriteSaved((id, favorite) => { setActors(current => current.map(actor => actor.id === id ? { ...actor, favorite } : actor)) }), [])

  useEffect(() => listenFavoriteSaved((id, favorite) => {
    setRealActorFilms(current => current.map(film => film.id === id ? { ...film, favorite } : film))
    setFilms(current => current.map(film => film.id === id ? { ...film, favorite } : film))
  }), [])

  useEffect(() => {
    document.title = t("THE ARCHIVE  私人影像收藏室")
  }, [locale])

  useEffect(() => {
    document.documentElement.dataset.reduceMotion = String(settings.reduceMotion)
    document.documentElement.dataset.thinScrollbar = String(settings.thinScrollbar)
    document.documentElement.style.setProperty('--film-cover-ratio', ratioCss(displayPreferences.value.defaultFilmRatio))
    document.documentElement.style.setProperty('--film-detail-cover-width', displayPreferences.value.defaultFilmRatio === '5:7' ? '66.666667%' : '100%')
    document.documentElement.style.setProperty('--actor-photo-ratio', ratioCss(displayPreferences.value.actorRatio, true))
    document.documentElement.style.setProperty('--film-cover-fit', settings.filmCoverFit)
    document.documentElement.style.setProperty('--actor-photo-fit', settings.actorPhotoFit)
  }, [settings, displayPreferences.value.defaultFilmRatio, displayPreferences.value.actorRatio])

  const visible = useMemo(() => {
    if (realFilmsAvailable) return []
    const term = query.trim().toLocaleLowerCase()
    return films.filter(film => {
    return (!term || film.title.toLocaleLowerCase().includes(term))
      && (library === 'all' || film.library === library)
      && (ratingFilter === 'all' || film.rating === Number(ratingFilter))
      && (!favoritesOnly || film.favorite)
      && selectedTags.every(tag => film.tags.includes(tag))
    }).sort((a, b) => compareFilms(a, b, sort, direction))
  }, [films, query, library, ratingFilter, favoritesOnly, selectedTags, sort, direction])
  const tagCounts = useMemo(() => tagFilmCounts(visible), [visible])
  const visibleIds = useMemo(() => new Set(visible.map(film => film.id)), [visible])

  // Selection is always limited to the films still visible after a search or filter change.
  useEffect(() => {
    setSelectedIds(current => {
      const next = current.filter(id => visibleIds.has(id))
      return next.length === current.length ? current : next
    })
  }, [visibleIds])

  const demoRatio = filmRatioForSources([], library, displayPreferences.value, displayPreferences.overviewRatio ?? 'original')
  const nextDemoRatio = nextRatio(filmRatios, demoRatio)
  const resetFilters = () => { setQuery(''); setRatingFilter('all'); setFavoritesOnly(false); setSelectedTags([]) }
  const toggleTagFilter = (tag: string) => setSelectedTags(current => current.includes(tag) ? current.filter(item => item !== tag) : [...current, tag])
  const activeFilterCount = Number(query.trim().length > 0) + Number(library !== 'all') + Number(ratingFilter !== 'all') + Number(favoritesOnly) + selectedTags.length
  const selectedCount = selectedIds.filter(id => visibleIds.has(id)).length
  const bulkTagValue = tags.includes(bulkTag) ? bulkTag : (tags[0] ?? '')
  const detail = films.find(film => film.id === detailId) ?? null
  const libraryCount = new Set(films.map(film => film.library)).size
  const drawPoolCount = useMemo(() => getDrawPool(actors).length, [actors])
  const issueSources = useMemo(() => [...new Set(films.map(film => film.library))].sort((a, b) => a.localeCompare(b, 'zh-CN')), [films])
  const closeDetail = useCallback(() => setDetailId(null), [])
  const closeTagManager = useCallback(() => setTagManagerOpen(false), [])
  const updateFilm = useCallback((id: string, patch: Partial<Film>) => {
    setFilms(current => current.map(film => film.id === id ? { ...film, ...patch } : film))
  }, [])
  const updateActor = useCallback(async (id: string, patch: Partial<Actor>) => {
    if (realFilmsAvailable) {
      if (patch.favorite !== undefined) await setActorFavorite(id, patch.favorite)
      else await saveActorProfile(id, { birthMonth: patch.birthMonth ?? '', height: patch.height ?? '', cup: patch.cup ?? '', bust: patch.bust ?? '', waist: patch.waist ?? '', hips: patch.hips ?? '' }, patch.aliases)
    }
    setActors(current => current.map(actor => actor.id === id ? { ...actor, ...patch } : actor))
    if (realFilmsAvailable) notifyRealLibraryChanged()
  }, [])
  const updateSettings = useCallback((patch: Partial<PreviewSettings>) => {
    if (['reduceMotion', 'thinScrollbar', 'filmCoverRatio', 'actorPhotoRatio', 'filmCoverFit', 'actorPhotoFit'].some(key => key in patch)) preferencesRevision.current++
    setSettings(current => ({ ...current, ...patch }))
  }, [])
  const resetUserData = useCallback(() => {
    try { clearOverviewRatio() } catch { /* Keep the completed data reset usable if browser storage is unavailable. */ }
    invalidateCovers()
    setDetailId(null); setActorDetailOpen(false); setActorEntryId(null); setActorEntryOrigin('actors')
    setTagManagerOpen(false); setTagEditing(false); setSelectedIds([]); setSelectedTags([]); setBulkNotice(''); setBulkTag('')
    setFavoritesOnly(false); setTags([]); setHistoryEvents([])
    preferencesRevision.current++
    setSettings({ ...initialSettings }); setQuery(''); setActorQuery(''); setMissingActorPhotosOnly(false); setLibrary('all'); setRatingFilter('all')
    setSort('createdAt'); setDirection('desc'); setDensity('standard')
    setEnabledSourceCount(0); setHistoryActiveDays(null); setStartupMessage('')
    // Unmount component-local manual drafts, filters and drawn actor snapshots.
    setActors([]); setRealActorFilms([]); setFilms([]); setLoading(true)
    setDataRevision(value => value + 1)
    notifyRealLibraryChanged()
  }, [])
  const clearHistory = useCallback(() => {
    setHistoryEvents([])
    setFilms(current => filmsWithLaunchHistory(current, []))
  }, [])

  const toggleTagEditing = () => {
    setTagEditing(current => !current)
    setSelectedIds([])
    setBulkNotice('')
    setDetailId(null)
  }
  const toggleFilmSelection = (id: string) => {
    setSelectedIds(current => current.includes(id) ? current.filter(item => item !== id) : [...current, id])
    setBulkNotice('')
  }
  const renameTag = (name: string, newName: string) => {
    setTags(current => current.map(tag => tag === name ? newName : tag))
    setSelectedTags(current => current.map(tag => tag === name ? newName : tag))
    setBulkTag(current => current === name ? newName : current)
    setFilms(current => current.map(film => ({ ...film, tags: film.tags.map(tag => tag === name ? newName : tag) })))
  }
  const deleteTag = (tag: string) => {
    setTags(current => current.filter(item => item !== tag))
    setSelectedTags(current => current.filter(item => item !== tag))
    setFilms(current => current.map(film => ({ ...film, tags: film.tags.filter(item => item !== tag) })))
  }
  const applyBulkTag = (action: 'add' | 'remove') => {
    if (!bulkTagValue || selectedCount === 0) return
    const ids = new Set(selectedIds.filter(id => visibleIds.has(id)))
    setFilms(current => current.map(film => {
      if (!ids.has(film.id)) return film
      return { ...film, tags: action === 'add'
        ? (film.tags.includes(bulkTagValue) ? film.tags : [...film.tags, bulkTagValue])
        : film.tags.filter(tag => tag !== bulkTagValue) }
    }))
    setBulkNotice(uiMessage(action === 'add' ? "已为 {0} 部影片添加标签“{1}”。" : "已从 {0} 部影片移除标签“{1}”。", [ids.size, bulkTagValue]))
    setSelectedIds([])
  }

  return <div className={realFilmsAvailable ? "desktop-window" : undefined}><TextEditMenu /><WindowTitleBar /><WindowCloseDialog /><div ref={appElement} data-motion={motionEnabled} className={`app-shell ${navigationCollapsed ? 'navigation-collapsed' : ''}`}>
    <CometTrail reduceMotion={settings.reduceMotion} />
    <Navigation activePage={activePage} collapsed={navigationCollapsed} onToggle={() => setNavigationCollapsed(value => !value)} onNavigate={navigate} />
    <PageFrame>
      {!(activePage === 'actors' && actorDetailOpen) && <header className="topbar"><div className="breadcrumb">COLLECTION <span>/</span> {activePage === 'films' ? 'FILMS' : activePage === 'actors' ? 'ACTORS' : activePage === 'actorFlip' ? 'ACTOR DRAW' : activePage === 'history' ? 'HISTORY' : activePage === 'issues' ? 'ISSUES' : 'SETTINGS'}</div>{activePage === 'actorFlip' && <div className="flip-pool-note"><span aria-hidden="true">✦</span>{t(realFilmsAvailable ? "从演员库中的 {0} 位演员中抽取" : "从演示资料中的 {0} 位演员中抽取", [loading || actorLoadError ? '…' : drawPoolCount])}</div>}{activePage === 'settings' && <ConfigurationDirectory desktop={realFilmsAvailable} path={realFilmsAvailable ? configurationDirectory : 'C:\Users\Public\Aurelia\config.json'} />}{(activePage === 'films' || activePage === 'actors') && <label className="global-search"><span className="search-symbol" aria-hidden="true">⌕</span><input type="search" value={activePage === 'films' ? query : actorQuery} onChange={event => activePage === 'films' ? setQuery(event.target.value) : setActorQuery(event.target.value)} placeholder={activePage === 'films' ? t("搜索影片名或编号") : t("搜索演员姓名或别名")} aria-label={activePage === 'films' ? t("搜索影片名或编号") : t("搜索演员姓名或别名")} /><kbd>{t("搜索")}</kbd></label>}{activePage === 'films' && <div className="film-source-overview"><span className="history-overview-dot" aria-hidden="true" />{t("已启用片库 {0}个", [realFilmsAvailable ? enabledSourceCount : libraryCount])}</div>}{activePage === 'actors' && <div className="film-source-overview"><span className="history-overview-dot" aria-hidden="true" />{t("照片齐全率 {0}% ·", [loading || actorLoadError ? '—' : photoSummary.percent])}<button type="button" className="actor-photo-filter-link" disabled={loading || Boolean(actorLoadError) || photoSummary.missing === 0} aria-pressed={missingActorPhotosOnly} title={missingActorPhotosOnly ? t("取消照片缺失筛选") : t("筛选照片缺失的演员")} onClick={() => setMissingActorPhotosOnly(value => !value)}>{t("缺失 {0}位", [loading || actorLoadError ? '—' : photoSummary.missing])}</button></div>}{activePage === 'history' && <div className="film-source-overview"><span className="history-overview-dot" aria-hidden="true" />{t("近 30 天有播放记录 {0}天", [historyActiveDays ?? '—'])}</div>}</header>}
      {realFilmsAvailable && (startupBusy || startupError) && <div className={`library-startup-feedback ${startupError ? 'is-error' : ''}`} role={startupError ? 'alert' : 'status'}>{startupBusy ? t("正在读取保存的资料，请稍候…") : t(startupMessage)}{startupError && <button type="button" className="settings-action" disabled={startupBusy} onClick={() => void prepareSession().then(() => notifyRealLibraryChanged())}>{t("重试读取资料")}</button>}</div>}
      {sessionReady ? <>
      <MotionPage active={activePage === 'films'} enabled={motionEnabled}>{realFilmsAvailable ? <RealFilmLibrary display={displayPreferences} key={dataRevision} films={realActorFilms} tags={tags} loading={loading} error={catalogLoadError} onFilmsChange={setRealActorFilms} onTagsChange={setTags} query={query} onClearQuery={clearFilmQuery} density={density} onDensityChange={setDensity} active={activePage === 'films'} onOpenActor={openFilmActor} onOpenScanSettings={() => { navigate('settings'); setScanSettingsRequested(true) }} /> :
      <div className="page-content"><section className="library-section" aria-labelledby="library-heading">
        <ScrollToTopButton />
        <div className="section-heading"><div><h1 id="library-heading">{t("影片库")}</h1><p className="collection-summary">{t("共 {0}部影片 · 标签 {1}个 · 收藏 {2}部", [films.length, tags.length, films.filter(film => film.favorite).length])}</p></div><div className="library-actions"><button type="button" onClick={() => setTagManagerOpen(true)}>{t("标签编辑")}</button><button className={tagEditing ? 'active' : ''} type="button" aria-pressed={tagEditing} onClick={toggleTagEditing}>{tagEditing ? t("退出影片管理") : t("影片管理")}</button></div></div>
        <div className="controls"><div className="filter-row"><div className="control-label">{t("筛选")}</div><label className="select-wrap"><span>{t("片库")}</span><AppSelect popupSizing="content" value={library} onChange={event => setLibrary(event.target.value)} aria-label={t("片库")}><option value="all">{t("全部片库")}</option><option value="主收藏">主收藏</option><option value="备用片库">备用片库</option></AppSelect></label><label className="select-wrap"><span>{t("评分")}</span><AppSelect popupSizing="content" value={ratingFilter} onChange={event => setRatingFilter(event.target.value as RatingFilter)} aria-label={t("评分")}><option value="all">{t("不限")}</option>{[5, 4, 3, 2, 1].map(stars => <option key={stars} value={String(stars)}>{t("{0}星", [stars])}</option>)}</AppSelect></label><FilterChip active={favoritesOnly} onClick={() => setFavoritesOnly(value => !value)}>✦ <span>{t("已收藏")}</span></FilterChip><div className="filter-divider" /><TagFilters showCounts={!tagEditing} counts={tagCounts} tags={tags} selected={selectedTags} onToggle={toggleTagFilter} onReorder={setTags} /></div>
          <div className="toolbar"><div className="result-count" aria-live="polite"><strong>{visible.length.toString().padStart(2, '0')}</strong><span>{t("部影片")}</span>{activeFilterCount > 0 && <button type="button" className="clear-link" onClick={resetFilters}>{t("清除筛选 · {0}", [activeFilterCount])}</button>}</div><div className="toolbar-actions"><label className="sort-label"><span>{t("排序")}</span><AppSelect value={sort} onChange={event => setSort(event.target.value as SortField)} aria-label={t("排序依据")}><option value="code">{t("按影片编号")}</option><option value="title">{t("按影片名称")}</option><option value="rating">{t("按个人评分")}</option><option value="createdAt">{t("按创建时间")}</option><option value="modifiedAt">{t("按修改时间")}</option><option value="lastLaunchedAt">{t("按最近播放时间")}</option></AppSelect></label><button className="direction-button" type="button" onClick={() => setDirection(value => value === 'asc' ? 'desc' : 'asc')} aria-label={t(direction === 'asc' ? "当前为升序，点击切换方向" : "当前为降序，点击切换方向")} title={direction === 'asc' ? t("升序") : t("降序")}><ArrowIcon direction={direction === 'asc' ? 'up' : 'down'} /></button><div className="toolbar-divider" /><RatioButton label={t("影片封面比例")} value={ratioLabel(demoRatio)} next={ratioLabel(nextDemoRatio)} disabled={displayPreferences.busy || !displayPreferences.loaded} onClick={() => library === 'all' ? displayPreferences.setOverview(nextDemoRatio) : void displayPreferences.setFilm(library, nextDemoRatio)} /><div className="toolbar-divider" /><fieldset className="density-control"><legend>{t("卡片密度")}</legend>{([['comfortable', t("舒适"), '▣'], ['standard', t("标准"), '▥'], ['compact', t("紧凑"), '▦']] as const).map(([value, label, icon]) => <button key={value} type="button" className={density === value ? 'active' : ''} aria-label={t("{0}密度", [label])} aria-pressed={density === value} title={t("{0}密度", [label])} onClick={() => setDensity(value)}>{icon}</button>)}</fieldset></div></div>
        </div>
        {tagEditing && <div className="bulk-bar"><div className="bulk-count"><span className="bulk-check" aria-hidden="true">✓</span>{t("已选")}<strong>{selectedCount}</strong> {t("部影片")}<span className="bulk-visible">{t("／当前显示 {0}部", [visible.length])}</span></div><div className="bulk-actions"><label>{t("目标标签")}<AppSelect value={bulkTagValue} onChange={event => setBulkTag(event.target.value)} disabled={tags.length === 0} aria-label={t("批量操作目标标签")}>{tags.length === 0 && <option value="">{t("暂无标签")}</option>}{tags.map(tag => <option key={tag} value={tag}>{tag}</option>)}</AppSelect></label><button type="button" disabled={!selectedCount || !bulkTagValue} onClick={() => applyBulkTag('add')}>{t("添加标签")}</button><button type="button" disabled={!selectedCount || !bulkTagValue} onClick={() => applyBulkTag('remove')}>{t("移除标签")}</button><button type="button" disabled={!selectedCount} onClick={() => { const ids = new Set(selectedIds.filter(id => visibleIds.has(id))); if (!window.confirm(t("确定删除 {0} 部演示影片吗？", [ids.size]))) return; setFilms(current => current.filter(film => !ids.has(film.id))); setSelectedIds([]); setBulkNotice(uiMessage("已删除 {0} 部影片。", [ids.size])) }}>{t("删除影片")}</button></div>{bulkNotice && <span className="bulk-notice" role="status">{t(bulkNotice)}</span>}</div>}
        {loading ? <div className="loading-state" role="status">{t("正在载入影片…")}</div> : loadError ? <div className="empty-state" role="alert"><h3>{t("影片暂时无法载入")}</h3><p>{t("请刷新页面重试。")}</p></div> : visible.length === 0 ? <EmptyState onReset={resetFilters} /> : <div className={`film-grid density-${density}`}>{visible.map(film => <FilmCard coverRatio={ratioCss(filmRatioForSources([film.library], library, displayPreferences.value, displayPreferences.overviewRatio))} key={film.id} film={film} editing={tagEditing} selected={selectedIds.includes(film.id)} onOpen={() => tagEditing ? toggleFilmSelection(film.id) : setDetailId(film.id)} />)}</div>}
      </section></div>}</MotionPage>
      <MotionPage className="page-content" active={activePage === 'actors'} enabled={motionEnabled}>{actorLoadError && <p className="actor-edit-error" role="alert">{actorLoadError}</p>}{loading ? <div className="loading-state" role="status">{t("正在载入演员…")}</div> : <ActorLibrary display={displayPreferences} key={dataRevision} actors={actors} films={films} query={actorQuery} missingPhotosOnly={missingActorPhotosOnly} onClearPhotoFilter={clearActorPhotoFilter} detailOpen={actorDetailOpen} initialDetailId={actorEntryId} onClearQuery={clearActorQuery} onDetailViewChange={changeActorDetail} onDetailBack={backFromActor} onPortraitBack={backFromPortrait} onUpdateActor={updateActor} onOpenFilm={setDetailId} />}</MotionPage>
      <MotionPage className="page-content" active={activePage === 'history'} enabled={motionEnabled} lazy>{realFilmsAvailable ? <RealPlaybackHistory onActivityDays={setHistoryActiveDays} key={dataRevision} active={activePage === 'history'} /> : <PlaybackHistory onActivityDays={setHistoryActiveDays} films={films} events={historyEvents} loading={loading} active={activePage === 'history'} onClear={clearHistory} onOpenFilm={setDetailId} />}</MotionPage>
      <MotionPage className="page-content" active={activePage === 'issues'} enabled={motionEnabled} lazy><ProblemCenter key={dataRevision} sources={issueSources} active={activePage === 'issues'} /></MotionPage>
      <MotionPage className="page-content" active={activePage === 'settings'} enabled={motionEnabled}><SettingsPage key={dataRevision} settings={settings} onUpdateSettings={updateSettings} onDataCleared={resetUserData} scanGuideActive={scanGuideActive && activePage === 'settings'} /></MotionPage>
      <MotionPage className="page-content" active={activePage === 'actorFlip'} enabled={motionEnabled}><ActorFlip key={dataRevision} entranceSession={flipEntranceSession.current} active={activePage === 'actorFlip'} reduceMotion={settings.reduceMotion} actors={actors} films={films} loading={loading} error={actorLoadError} onOpenActor={(id, animate) => openActor(id, 'actorFlip', animate)} onOpenActorLibrary={() => navigate('actors')} /></MotionPage>
      </> : <div className="loading-state" role="status">{t("正在载入片库…")}</div>}
    </PageFrame>
    {realFilmsAvailable ? <RealFilmDetail film={realActorFilms.find(film => film.id === detailId) ?? null} availableTags={tags}
      onTags={async (id, names) => { await setRealFilmTags(id, names); notifyRealLibraryChanged() }} onClose={closeDetail} onOpenActor={id => openActor(id, activePage === 'actors' ? actorEntryOrigin : activePage)} />
      : <FilmDetail film={detail} availableTags={tags} onUpdate={updateFilm} onClose={closeDetail} />}
    {tagManagerOpen && <TagManager onRename={renameTag} tags={tags} onAdd={tag => setTags(current => [...current, tag])} onDelete={deleteTag} onClose={closeTagManager} />}
    <EmbeddedPlayerHost reduceMotion={settings.reduceMotion} />
  </div></div>
}

export default App
