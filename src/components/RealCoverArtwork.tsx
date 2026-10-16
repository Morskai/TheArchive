import { t, useLocale } from '../i18n/react'
import { useEffect, useRef, useState } from 'react'
import { coverCache, coverMessage } from '../data/covers'

export function useRealArtwork(filmId: string, large: boolean) {
  const root = useRef<HTMLDivElement>(null)
  const priority = useRef(1)
  const [nearby, setNearby] = useState(false)
  const [revision, setRevision] = useState(0)
  const key = JSON.stringify([filmId, large])
  const [image, setImage] = useState<{ url: string; key: string } | null>(() => {
    const url = coverCache.peek(filmId, large)
    return url ? { url, key } : null
  })
  const [notice, setNotice] = useState('')
  useEffect(() => {
    const node = root.current
    if (!node) return
    const visible = new IntersectionObserver(entries => { priority.current = entries[0].isIntersecting ? 0 : 1 })
    const nearby = new IntersectionObserver(entries => setNearby(entries[0].isIntersecting))
    visible.observe(node); nearby.observe(node)
    return () => { visible.disconnect(); nearby.disconnect() }
  }, [])
  useEffect(() => {
    const refresh = (event: Event) => {
      const filmIds = (event as CustomEvent<{ filmIds?: string[] }>).detail?.filmIds
      if (filmIds && !filmIds.includes(filmId)) return
      setImage(null); setRevision(value => value + 1)
    }
    window.addEventListener('real-covers-invalidated', refresh)
    return () => window.removeEventListener('real-covers-invalidated', refresh)
  }, [filmId])
  // Loaded images keep their DOM source while offscreen. Clear only when their
  // cache owner actually evicts/revokes them, or explicit cache clearing/source changes invalidate.
  useEffect(() => coverCache.subscribe(filmId, large, url => {
    setImage(current => current?.url === url ? null : current)
  }), [filmId, large])
  useEffect(() => {
    if (!nearby) return
    let active = true
    setNotice('')
    const lease = coverCache.acquire(filmId, large, () => active, () => priority.current)
    setImage(lease.url ? { url: lease.url, key } : null)
    lease.result.then(url => {
      if (!active || !url) return
      setImage({ url, key })
      window.dispatchEvent(new CustomEvent('real-cover-status', { detail: { filmId, available: true } }))
    }).catch(error => { if (active) {
      setImage(null); setNotice(coverMessage(error))
      window.dispatchEvent(new CustomEvent('real-cover-status', { detail: { filmId, available: false } }))
    } })
    return () => { active = false; lease.release() }
  }, [filmId, large, nearby, key, revision])
  const url = image?.key === key ? image.url : null
  return { root, url, notice, failed: () => { setImage(null); setNotice('图片暂无法显示') } }
}

export function RealCoverArtwork({ filmId, large = false }: { filmId: string; large?: boolean }) {
  useLocale()
  const { root, url, notice, failed } = useRealArtwork(filmId, large)
  return <div ref={root} className={`film-art art-0 real-cover-art ${large ? 'film-art-large' : ''} ${url ? '' : 'art-missing'}`} aria-hidden={notice ? undefined : true}>
    {url ? <img src={url} alt="" decoding="async" onError={failed} /> : <><span className="missing-symbol">◇</span><span className="missing-label">{t("暂无封面")}</span></>}
    {notice && <span className="real-cover-notice" role="status">{t(notice)}</span>}
  </div>
}
