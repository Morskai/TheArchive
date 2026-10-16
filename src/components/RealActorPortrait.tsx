import { t, useLocale } from '../i18n/react'
import { castNameLabel } from '../data/tagPresentation'
import type { Actor } from '../data/actors'
import { useRealArtwork } from './RealCoverArtwork'

export function RealFilmActorPhoto({ actor, filmId, videoId, onOpen }: { actor: { id: string; name: string }; filmId: string; videoId: string; onOpen?: () => void }) {
  useLocale()
  const { root, url, failed } = useRealArtwork(`film-actor:${JSON.stringify([filmId, videoId, actor.id])}`, false)
  const content = <><div ref={root} className="actor-avatar">
    {url ? <img src={url} alt={t("{0}头像", [actor.name])} decoding="async" onError={failed} /> : <span role="img" aria-label={t("{0}暂无照片", [actor.name])}>◇</span>}
  </div><span className="actor-name" title={actor.name}>{castNameLabel(actor.name)}</span></>
  return onOpen ? <button className="actor-row actor-link" type="button" onClick={onOpen} aria-label={t("查看{0}演员详情", [actor.name])}>{content}</button> : <div className="actor-row">{content}</div>
}

export function RealActorPortrait({ actor, large }: { actor: Actor; large: boolean }) {
  useLocale()
  const { root, url, failed } = useRealArtwork(`actor:${actor.id}`, large)
  return <div ref={root} className={`actor-portrait real-actor-portrait ${large ? 'actor-portrait-large' : ''}`}>
    {url ? <img className="real-actor-photo" src={url} alt={t("{0}照片", [actor.name])} decoding="async" onError={failed} /> : <div className="actor-portrait-placeholder" role="img" aria-label={t("{0}暂无照片", [actor.name])}><span aria-hidden="true">◇</span><small>{t("暂无照片")}</small></div>}
    {!large && <span className="portrait-hint" aria-hidden="true">{t("查看资料 ↗")}</span>}
  </div>
}
