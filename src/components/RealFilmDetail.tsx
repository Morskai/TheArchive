import { t, useLocale } from '../i18n/react'
import { videoResolutionLabel, type VideoResolution } from '../data/deliveryPresentation'
import { useEffect, useRef, useState } from 'react'
import { type FilmCardView } from './FilmCard'
import { setRealRating, type RealFilm } from '../data/realFilms'
import { launchFilmVideo, openFilmFolder, setPreferredVideo, notifyRealLibraryChanged, playbackError, loadVideoSize, loadVideoResolution, loadDirectoryFacts, type DirectoryFacts } from '../data/playback'
import { displayLibraryPath } from '../data/libraryPath'
import { localLaunchTime } from '../data/localDates'
import { defaultFilmVideo, filmDisplayCode, hasAmbiguousFilmCode, filmSourceNames, videoNameAndFormat, videoSizeLabel } from '../data/realFilmPresentation'
import { RealFilmActorPhoto } from './RealActorPortrait'
import { useManualValue } from './useManualValue'
import { useFilmFavorite } from './useFilmFavorite'
import { FilmCastEditor } from './FilmCastEditor'
import { FilmTitleEditor, type FilmTitleEditorHandle } from './FilmTitleEditor'
import { RealVersionCoverArtwork } from './RealVersionCoverArtwork'

export function realFilmCardView(film: RealFilm): FilmCardView {
  return {
    id: film.id,
    code: filmDisplayCode(film),
    codePlaceholder: hasAmbiguousFilmCode(film),
    title: film.title,
    rating: film.rating,
    tags: film.tags,
    favorite: film.favorite,
    hasCover: film.has_cover ?? false,
    artwork: 0,
    realFilmId: film.id,
  }
}

interface RealFilmDetailProps {
  film: RealFilm | null
  availableTags: string[]
  onTags: (id: string, names: string[]) => Promise<void>
  onClose: () => void
  onOpenActor?: (id: string) => void
}

export function RealFilmDetail({ film, availableTags, onTags, onClose, onOpenActor }: RealFilmDetailProps) {
  useLocale()
  const favorite = useFilmFavorite(film?.id, film?.favorite ?? false)
  const rating = useManualValue('film-rating:' + (film?.id ?? 'none'), film?.rating ?? null, value => setRealRating(film!.id, value))
  const preferred = useManualValue('film-preferred:' + (film?.id ?? 'none'), film?.preferred_video_id ?? null, (value, committed) => setPreferredVideo(film!.id, value ?? committed ?? film!.videos[0].id, value !== null))
  const titleEditor = useRef<FilmTitleEditorHandle>(null)
  const closeDetail = () => { titleEditor.current?.finish(); onClose() }
  const drawerRef = useRef<HTMLElement>(null)
  const closeRef = useRef<HTMLButtonElement>(null)
  const castEditButton = useRef<HTMLButtonElement>(null)
  const [castEditing, setCastEditing] = useState(false)
  const castEditingRef = useRef(false)
  castEditingRef.current = castEditing
  const [busy, setBusy] = useState(false)
  const [saveError, setSaveError] = useState('')
  const [operationNotice, setOperationNotice] = useState('')
  const guard = useRef(false)
  const activeFilm = useRef<string | null>(null)
  activeFilm.current = film?.id ?? null
  const [selectedVersionId, setSelectedVersionId] = useState<string | null>(null)
  const [resolutions, setResolutions] = useState<Record<string, VideoResolution | null>>({})
  const [videoSizes, setVideoSizes] = useState<Record<string, number>>({})
  const [directory, setDirectory] = useState<DirectoryFacts | null>(null)
  const [directoryError, setDirectoryError] = useState('')
  const [feedback, setFeedback] = useState<{ action: 'play' | 'folder'; phase: 'pending' | 'success' } | null>(null)
  const activeVersionId = film?.videos.some(video => video.id === selectedVersionId) ? selectedVersionId : film ? defaultFilmVideo(film) : null
  const videoIds = film?.videos.map(video => video.id).join('|') ?? ''
  useEffect(() => {
    let active = true
    setDirectory(null); setDirectoryError('')
    if (film && activeVersionId) loadDirectoryFacts(film.id, activeVersionId)
      .then(facts => { if (active) setDirectory(facts) })
      .catch(error => { if (active) setDirectoryError(playbackError(error)) })
    return () => { active = false }
  }, [film?.id, activeVersionId, videoIds])
  useEffect(() => {
    if (feedback?.phase !== 'success') return
    const timer = window.setTimeout(() => setFeedback(null), 1800)
    return () => window.clearTimeout(timer)
  }, [feedback])
  useEffect(() => {
    let active = true
    setVideoSizes({}); setResolutions({})
    if (film) {
      for (const video of film.videos) void loadVideoResolution(film.id, video.id)
        .then(value => { if (active) setResolutions(current => ({ ...current, [video.id]: value })) })
        .catch(() => { /* Optional property failures leave the established display unchanged. */ })
      for (const video of film.videos) loadVideoSize(film.id, video.id)
        .then(size => { if (active) setVideoSizes(current => ({ ...current, [video.id]: size })) })
        .catch(() => { /* Unknown size remains explicit; playback reports access errors. */ })
    }
    return () => { active = false }
  }, [film?.id, videoIds])
  useEffect(() => {
    setSelectedVersionId(film ? defaultFilmVideo(film) : null)
    setSaveError(''); setOperationNotice('')
    setFeedback(null)
    setCastEditing(false)
  }, [film?.id])

  useEffect(() => {
    if (!film) return
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    closeRef.current?.focus({ preventScroll: true })
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        if (castEditingRef.current) { event.preventDefault(); setCastEditing(false); castEditButton.current?.focus({ preventScroll: true }) }
        else { titleEditor.current?.finish(); onClose() }
        return
      }
      if (event.key !== 'Tab' || !drawerRef.current) return
      const focusable = [...drawerRef.current.querySelectorAll<HTMLElement>('button:not(:disabled), select:not(:disabled), input:not(:disabled), textarea:not(:disabled)')]
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => { window.removeEventListener('keydown', onKeyDown); previousFocus?.focus({ preventScroll: true }) }
  }, [film?.id, onClose])

  const save = async (action: () => Promise<void>, operation?: 'play' | 'folder') => {
    if (guard.current) return
    guard.current = true
    const id = film?.id
    setBusy(true)
    setSaveError('')
    setOperationNotice('')
    setFeedback(operation ? { action: operation, phase: 'pending' } : null)
    try { await action(); if (operation && activeFilm.current === id) setFeedback({ action: operation, phase: 'success' }) }
    catch (error) { if (activeFilm.current === id) { setFeedback(null); setOperationNotice(''); setSaveError(playbackError(error)) } }
    finally { guard.current = false; setBusy(false) }
  }

  if (!film) return null
  const card = realFilmCardView(film)
  const sources = filmSourceNames(film)
  const directoryPath = directory?.path ?? film.videos.find(video => video.id === activeVersionId)?.directory_path
  const isPreferred = activeVersionId === preferred.value
  const play = () => save(async () => {
    if (!activeVersionId) return
    const outcome = await launchFilmVideo(film.id, activeVersionId)
    if (activeFilm.current === film.id) setOperationNotice(outcome.historySaved
      ? ''
      : '播放器启动请求已成功，但历史保存失败。软件不会自动再次启动。')
    if (outcome.historySaved) notifyRealLibraryChanged()
  }, 'play')
  const actionIcon = (action: 'play' | 'folder') => feedback?.action === action
    ? <svg className={`detail-action-feedback ${feedback.phase === 'pending' ? 'is-pending' : ''}`} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true">{feedback.phase === 'pending' ? <path d="M16 10a6 6 0 1 1-6-6" /> : <path d="m4 10 4 4 8-8" />}</svg>
    : null

  return <div className="detail-layer">
    <button className="detail-backdrop" type="button" aria-label={t("关闭详情")} onClick={closeDetail} />
    <section className="detail-drawer" ref={drawerRef} role="dialog" aria-modal="true" aria-labelledby="detail-title">
      <div className="detail-top"><span>{t("影片详情")}</span><button className="close-button" ref={closeRef} type="button" onClick={closeDetail} aria-label={t("关闭详情")}>×</button></div>
      {activeVersionId && <RealVersionCoverArtwork filmId={film.id} videoId={activeVersionId} />}

      <div className="detail-identity"><FilmTitleEditor key={film.id} ref={titleEditor} film={film} code={card.code} />
        <div className="rating-line"><span className="rating-caption">{t("个人评分")}</span><div className="rating-stars" role="group" aria-label={t("个人评分，五颗星")}>{[1, 2, 3, 4, 5].map(value => <button key={value} type="button" aria-busy={rating.saving} aria-label={t("评为{0}星", [value])} aria-pressed={rating.value === value} onClick={() => rating.set(rating.value === value ? null : value)}><span className={`star-shape ${rating.value !== null && value <= rating.value ? 'filled' : rating.value !== null && value - rating.value === 0.5 ? 'real-half-star' : ''}`} aria-hidden="true" /></button>)}</div><span className="rating-number" aria-live="polite">{rating.value === null ? t("未评分") : `${rating.value.toFixed(1)}/5.0`}</span></div>
      </div>

      {saveError && <p className="real-save-error" role="alert">{t(saveError)}</p>}
      {rating.error && <p className="real-save-error" role="alert">{t("评分未保存，已恢复上次保存状态，请重试。")}</p>}
      {preferred.error && <p className="real-save-error" role="alert">{t("首选版本未保存，已恢复上次保存状态，请重试。")}</p>}
      {favorite.error && <p className="real-save-error" role="alert">{t("收藏未保存，已恢复上次保存状态，请重试。")}</p>}
      <div className="detail-section"><h3>{t("标签")}</h3>{availableTags.length === 0 ? <p className="real-detail-empty">{t("暂无标签，请在标签编辑中新建。")}</p> : <div className="detail-tags">{availableTags.map(tag => <button key={tag} type="button" disabled={busy} className={film.tags.includes(tag) ? 'selected' : ''} aria-pressed={film.tags.includes(tag)} onClick={() => save(() => onTags(film.id, film.tags.includes(tag) ? film.tags.filter(item => item !== tag) : [...film.tags, tag]))}>{tag}</button>)}</div>}</div>
      <div className="detail-section"><div className="film-cast-heading"><h3>{t("出演表")}</h3><button ref={castEditButton} type="button" aria-expanded={castEditing} aria-controls={castEditing ? 'film-cast-editor' : undefined} onClick={() => setCastEditing(value => !value)}>{castEditing ? t("完成编辑") : t("编辑")}</button></div>{film.actors?.length && activeVersionId ? <div className="detail-actor-list">{film.actors.map(actor => <RealFilmActorPhoto key={actor.id} actor={actor} filmId={film.id} videoId={activeVersionId} onOpen={onOpenActor ? () => onOpenActor(actor.id) : undefined} />)}</div> : <p className="real-detail-empty">{t("暂无出演演员。")}</p>}<FilmCastEditor key={film.id} filmId={film.id} actors={film.actors ?? []} editing={castEditing} /></div>
      <div className="detail-section"><h3>{t("文件信息")}</h3><dl className="detail-facts"><div className="real-directory-fact"><dt>{t("目录路径")}</dt><dd className="real-directory-path">{directoryPath ? displayLibraryPath(directoryPath) : directoryError ? t("暂时无法读取") : t("正在读取…")}</dd></div><div><dt>{t("来源片库")}</dt><dd>{sources}</dd></div><div><dt>{t("文件数量")}</dt><dd>{directory ? t("{0}个视频 · {1}张图片 · {2}个字幕文件", [directory.videos, directory.images, directory.subtitles]) : directoryError ? t("暂时无法读取") : t("正在读取…")}</dd></div><div><dt>{t("最近一次播放时间")}</dt><dd>{film.last_launched_at ? localLaunchTime(film.last_launched_at) : t("暂无记录")}</dd></div></dl>{directoryError && <p className="real-detail-empty">{t(directoryError)}</p>}</div>

      <div className="detail-section detail-version-section"><h3>{t("播放版本")}</h3><div className="version-list" role="group" aria-label={t("选择播放版本")}>{film.videos.map(video => { const info = videoNameAndFormat(video.observed_path); return <button type="button" className={`version-option real-version ${activeVersionId === video.id ? 'selected' : ''}`} key={video.id} disabled={busy} aria-pressed={activeVersionId === video.id} title={video.observed_path} onClick={() => { setSelectedVersionId(video.id); setOperationNotice('') }}><span className="version-main"><span className="real-version-name">{info.name}</span><span className="real-version-facts">{t(videoResolutionLabel(resolutions[video.id]))} · {t(videoSizeLabel(videoSizes[video.id]))} · {info.format === '格式未知' && !video.observed_path.split(/[\\/]/).at(-1)?.includes('.') ? t("格式未知") : info.format}</span>{preferred.value === video.id && <strong>{t("✓ 首选")}</strong>}</span></button> })}</div>
        <div className="version-secondary"><button type="button" disabled={!activeVersionId} aria-busy={preferred.saving} onClick={() => preferred.set(isPreferred ? null : activeVersionId)}>{isPreferred ? t("取消首选版本") : t("设为首选版本")}</button><button type="button" className={feedback?.action === 'folder' && feedback.phase === 'success' ? 'action-succeeded' : ''} aria-busy={feedback?.action === 'folder' && feedback.phase === 'pending'} aria-label={feedback?.action === 'folder' && feedback.phase === 'success' ? t("打开所在目录，请求成功") : t("打开所在目录")} disabled={busy || !activeVersionId} onClick={() => save(async () => { await openFilmFolder(film.id, activeVersionId!) }, 'folder')}>{t("打开所在目录")}{actionIcon('folder')}</button></div>
        <button className={`version-play ${feedback?.action === 'play' && feedback.phase === 'success' ? 'action-succeeded' : ''}`} type="button" aria-busy={feedback?.action === 'play' && feedback.phase === 'pending'} aria-label={feedback?.action === 'play' && feedback.phase === 'success' ? t("播放所选版本，启动请求成功") : t("播放所选版本")} disabled={busy || !activeVersionId} onClick={event => { if (event.detail <= 1) play() }}>{actionIcon('play') ?? <span className="play-icon" aria-hidden="true" />}{t("播放所选版本")}</button>
        {operationNotice && <p className="real-detail-empty" role="status">{t(operationNotice)}</p>}
        <button className={`detail-favorite ${favorite.favorite ? 'is-favorite' : ''}`} type="button" disabled={busy} aria-pressed={favorite.favorite} aria-busy={favorite.saving} onClick={favorite.toggle}><span className="detail-favorite-icon" aria-hidden="true">{favorite.favorite ? '✦' : '✧'}</span>{favorite.favorite ? t("已收藏") : t("收藏")}</button>
      </div>
    </section>
  </div>
}
