import { t, useLocale } from '../i18n/react'
import { filmSourceNames } from '../data/realFilmPresentation'
import { listenFilmManualSaved } from '../data/manualEvents'
import { listenFavoriteSaved } from '../data/filmFavoriteEvents'
import { memo, useCallback, useEffect, useMemo, useState } from 'react'
import { PlaybackHistory } from './PlaybackHistory'
import { RealFilmDetail, realFilmCardView } from './RealFilmDetail'
import { loadRealTags, setRealFilmTags } from '../data/realFilms'
import { loadLaunchHistory, clearLaunchHistory, notifyRealLibraryChanged, playbackError, type RealHistory } from '../data/playback'

export const RealPlaybackHistory = memo(function RealPlaybackHistory({ active = true, onActivityDays }: { active?: boolean; onActivityDays?: (count: number | null) => void }) {
  useLocale()
  const [history, setHistory] = useState<RealHistory>({ films: [], events: [], film_count: 0, source_count: 0 })
  const [tags, setTags] = useState<string[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [retry, setRetry] = useState(0)
  const [detailId, setDetailId] = useState<string | null>(null)
  useEffect(() => listenFilmManualSaved((id, patch) => { setHistory(current => ({ ...current, films: current.films.map(film => film.id === id ? { ...film, ...patch } : film) })) }), [])

  useEffect(() => listenFavoriteSaved((id, favorite) => {
    setHistory(current => ({ ...current, films: current.films.map(film => film.id === id ? { ...film, favorite } : film) }))
  }), [])
  const closeDetail = useCallback(() => setDetailId(null), [])
  useEffect(() => { if (!active) setDetailId(null) }, [active])
  useEffect(() => {
    let active = true
    let revision = 0
    const refresh = () => {
      const requested = ++revision
      return Promise.all([loadLaunchHistory(), loadRealTags()])
        .then(([rows, names]) => { if (active && requested === revision) { setHistory(rows); setTags(names); setError('') } })
        .catch(reason => { if (active && requested === revision) setError(playbackError(reason)) })
        .finally(() => { if (active && requested === revision) setLoading(false) })
    }
    void refresh()
    window.addEventListener('real-library-changed', refresh)
    return () => { active = false; window.removeEventListener('real-library-changed', refresh) }
  }, [retry])
  const clear = async () => {
    try { await clearLaunchHistory() }
    catch (reason) { throw playbackError(reason) }
    setHistory(current => ({ ...current, events: [], films: current.films.map(film => ({ ...film, last_launched_at: null })) }))
    notifyRealLibraryChanged()
  }
  const films = useMemo(() => history.films.map(film => ({ ...realFilmCardView(film), actor: '', library: filmSourceNames(film) })), [history.films])
  return <><PlaybackHistory onActivityDays={onActivityDays} films={films} events={history.events} loading={loading} error={t(error)} active={active} real onClear={clear} onOpenFilm={setDetailId} onRetry={() => { setLoading(true); setRetry(value => value + 1) }} />
    <RealFilmDetail film={active ? history.films.find(film => film.id === detailId) ?? null : null} availableTags={tags} onClose={closeDetail}
      onTags={async (id, names) => { await setRealFilmTags(id, names); notifyRealLibraryChanged() }} />
  </>
})
