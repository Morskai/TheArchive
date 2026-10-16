import { t, useLocale } from '../i18n/react'
import { useRealArtwork } from './RealCoverArtwork'
export function RealVersionCoverArtwork({ filmId, videoId }: { filmId: string; videoId: string }) {
  useLocale()
 const { root, url, notice, failed } = useRealArtwork(`film-version:${JSON.stringify([filmId, videoId])}`, true)
 return <div ref={root} className={`film-art art-0 real-cover-art film-art-large ${url?'':'art-missing'}`}>{url?<img src={url} alt="" decoding="async" onError={failed}/>:<><span className="missing-symbol">◇</span><span className="missing-label">{t("暂无封面")}</span></>}{notice&&<span className="real-cover-notice" role="status">{t(notice)}</span>}</div>
}
