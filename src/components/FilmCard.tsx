import { t, useLocale } from '../i18n/react'
import type { CSSProperties } from 'react'
import type { Film } from '../data/films'
import { RealCoverArtwork } from './RealCoverArtwork'

export type FilmCardView = Pick<Film, 'id' | 'code' | 'title' | 'rating' | 'tags' | 'favorite' | 'hasCover' | 'artwork'> & { realFilmId?: string; codePlaceholder?: boolean }

export function FilmArtwork({ film, large = false }: { film: Pick<FilmCardView, 'code' | 'title' | 'hasCover' | 'artwork' | 'realFilmId'>; large?: boolean }) {
  useLocale()
  if (film.realFilmId) return <RealCoverArtwork filmId={film.realFilmId} large={large} />
  return <div className={`film-art art-${film.artwork} ${large ? 'film-art-large' : ''} ${!film.hasCover ? 'art-missing' : ''}`} aria-hidden="true">
    {film.hasCover ? <><span className="art-halo" /><span className="art-arc" /><span className="art-line" /><span className="art-grain" /><span className="art-caption">THE ARCHIVE <i> / </i> {film.code}</span><span className="art-title">{film.title}</span></> : <><span className="missing-symbol">◇</span><span className="missing-label">{t("暂无封面")}</span></>}
  </div>
}

export function FilmCard({ film, onOpen, onPlay, editing = false, selected = false, sourceName, codePlaceholder = film.codePlaceholder ?? false, coverRatio }: { coverRatio?: string; film: FilmCardView; onOpen: () => void; onPlay?: () => void; editing?: boolean; selected?: boolean; sourceName?: string; codePlaceholder?: boolean }) {
  useLocale()
  const hasCode = film.code.trim().length > 0
  const displayedCode = codePlaceholder ? t(film.code) : film.code
  const codeLabel = hasCode ? `，${displayedCode}` : ''
  return <button className={`film-card ${hasCode ? '' : 'without-code'} ${editing ? 'editing' : ''} ${selected ? 'is-selected' : ''}`} type="button" onClick={onOpen} onContextMenu={event => {
    event.preventDefault()
    if (event.button === 2 && !editing) onPlay?.()
  }} aria-label={editing ? t(selected ? "取消选择影片：{0}{1}" : "选择影片：{0}{1}", [film.title, codeLabel]) : t(onPlay ? "查看影片详情：{0}{1}，右键播放" : "查看影片详情：{0}{1}", [film.title, codeLabel])} aria-pressed={editing ? selected : undefined}>
    <div className="card-cover" style={coverRatio ? { '--film-cover-ratio': coverRatio } as CSSProperties : undefined}><FilmArtwork film={film} />{film.favorite && <span className="favorite-mark" aria-label={t("已收藏")}>✦</span>}{editing ? <span className="card-select-mark" aria-hidden="true">{selected ? '✓' : ''}</span> : <span className="card-view">{t("查看详情")}<span aria-hidden="true">↗</span></span>}</div>
    <div className="card-meta">{hasCode && <div className="card-code-row"><div className={`card-code ${codePlaceholder ? 'real-code-placeholder' : ''}`} title={displayedCode}>{displayedCode}</div>{sourceName && <span className="card-source" title={sourceName}>{sourceName}</span>}</div>}<div className="card-title" title={film.title}>{film.title}</div>{!hasCode && sourceName && <span className="card-source" title={sourceName}>{sourceName}</span>}<div className="card-bottom"><span className="card-rating">{film.rating === null ? t("未评分") : <><span className="star-shape filled" aria-hidden="true" /> <span className="rating-value">{film.rating.toFixed(1)}</span></>}</span>{film.tags.includes('中文字幕') && <span className="card-tags" title="中文字幕">中文字幕</span>}</div></div>
  </button>
}
