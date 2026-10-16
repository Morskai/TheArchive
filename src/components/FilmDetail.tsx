import { t, useLocale } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import type { Film } from '../data/films'
import { FilmArtwork } from './FilmCard'
import demoActorPortrait from '../assets/demo-actor.svg'

interface FilmDetailProps {
  film: Film | null
  availableTags: string[]
  onUpdate: (id: string, patch: Partial<Film>) => void
  onClose: () => void
}

export function FilmDetail({ film, availableTags, onUpdate, onClose }: FilmDetailProps) {
  useLocale()
  const drawerRef = useRef<HTMLElement>(null)
  const closeRef = useRef<HTMLButtonElement>(null)
  const [selectedVersionId, setSelectedVersionId] = useState<string | null>(null)

  useEffect(() => {
    setSelectedVersionId(film?.preferredVersionId ?? film?.versions[0]?.id ?? null)
  }, [film?.id, film?.preferredVersionId])

  useEffect(() => {
    if (!film) return
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    closeRef.current?.focus()
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { onClose(); return }
      if (event.key !== 'Tab' || !drawerRef.current) return
      const focusable = [...drawerRef.current.querySelectorAll<HTMLElement>('button:not(:disabled)')]
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus() }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus() }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => { window.removeEventListener('keydown', onKeyDown); previousFocus?.focus() }
  }, [film?.id, onClose])

  if (!film) return null
  const activeVersionId = selectedVersionId ?? film.versions[0]?.id ?? null
  const isPreferred = activeVersionId === film.preferredVersionId
  const toggleTag = (tag: string) => onUpdate(film.id, { tags: film.tags.includes(tag) ? film.tags.filter(item => item !== tag) : [...film.tags, tag] })

  return <div className="detail-layer">
    <button className="detail-backdrop" type="button" aria-label={t("关闭详情")} onClick={onClose} />
    <section className="detail-drawer" ref={drawerRef} role="dialog" aria-modal="true" aria-labelledby="detail-title">
      <div className="detail-top"><span>{t("影片详情")}</span><button className="close-button" ref={closeRef} type="button" onClick={onClose} aria-label={t("关闭详情")}>×</button></div>
      <FilmArtwork film={film} large />

      <div className="detail-identity"><div className="detail-code">{film.code}</div><h2 id="detail-title">{film.title}</h2>
        <div className="rating-line"><span className="rating-caption">{t("个人评分")}</span><div className="rating-stars" role="group" aria-label={t("个人评分，五颗星")}>
          {[1, 2, 3, 4, 5].map(value => <button key={value} type="button" aria-label={t("评为{0}星", [value])} aria-pressed={film.rating === value} onClick={() => onUpdate(film.id, { rating: film.rating === value ? null : value })}><span className={`star-shape ${film.rating !== null && value <= film.rating ? 'filled' : ''}`} aria-hidden="true" /></button>)}
        </div><span className="rating-number" aria-live="polite">{film.rating === null ? t("未评分") : `${film.rating.toFixed(1)}/5.0`}</span></div>
      </div>

      <div className="detail-section"><h3>{t("标签")}</h3><div className="detail-tags">{availableTags.map(tag => <button key={tag} type="button" className={film.tags.includes(tag) ? 'selected' : ''} aria-pressed={film.tags.includes(tag)} onClick={() => toggleTag(tag)}>{tag}</button>)}</div></div>

      <div className="detail-section"><h3>{t("出演表")}</h3><div className="actor-row"><span className={`actor-avatar actor-${film.actor.charCodeAt(0) % 4}`}><img src={demoActorPortrait} alt={t("{0}头像", [film.actor])} /></span><span className="actor-name">{film.actor}</span></div></div>

      <div className="detail-section"><h3>{t("文件信息")}</h3><dl className="detail-facts"><div><dt>{t("文件目录")}</dt><dd>{film.directory ?? t("暂无资料")}</dd></div><div><dt>{t("文件大小")}</dt><dd>{film.fileSize ?? t("暂无资料")}</dd></div><div><dt>{t("来源片库")}</dt><dd>{film.library}</dd></div><div><dt>{t("最近一次播放时间")}</dt><dd>{film.lastLaunchedAt ?? t("暂无记录")}</dd></div></dl></div>

      <div className="detail-section detail-version-section"><h3>{t("播放版本")}</h3><div className="version-list" role="group" aria-label={t("选择播放版本")}>{film.versions.map(version => <button key={version.id} type="button" className={`version-option ${activeVersionId === version.id ? 'selected' : ''}`} aria-pressed={activeVersionId === version.id} onClick={() => setSelectedVersionId(version.id)}><span className="version-main"><span>{version.label} · {version.size} · {version.format}</span>{film.preferredVersionId === version.id && <strong>{t("✓ 首选")}</strong>}</span><small>{version.note}</small></button>)}</div>
        <div className="version-secondary"><button type="button" onClick={() => onUpdate(film.id, { preferredVersionId: isPreferred ? null : activeVersionId })}>{isPreferred ? t("取消首选版本") : t("设为首选版本")}</button><button type="button" disabled>{t("打开所在目录")}</button></div>
        <button className="version-play" type="button" disabled><span className="play-icon" aria-hidden="true" />{t("播放所选版本")}</button>
        <button className={`detail-favorite ${film.favorite ? 'is-favorite' : ''}`} type="button" aria-pressed={film.favorite} onClick={() => onUpdate(film.id, { favorite: !film.favorite })}><span className="detail-favorite-icon" aria-hidden="true">{film.favorite ? '✦' : '✧'}</span>{film.favorite ? t("已收藏") : t("收藏")}</button>
      </div>
    </section>
  </div>
}
