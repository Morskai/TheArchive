import { useCallback, useEffect, useRef, useState } from 'react'
import { invoke } from '../data/diagnostics'
import { realFilmsAvailable } from '../data/realFilms'
import { filmRatios, type ActorRatio, type DisplayPreferences, type FilmRatio } from '../data/displayRatios'

const defaults: DisplayPreferences = { defaultFilmRatio: 'original', actorRatio: 'original', libraryRatios: {} }
const overviewRatioKey = 'thearchive.film-overview-ratio'
function readOverviewRatio(): FilmRatio {
  try {
    const saved = localStorage.getItem(overviewRatioKey)
    return filmRatios.includes(saved as FilmRatio) ? saved as FilmRatio : 'original'
  } catch { return 'original' }
}
export function clearOverviewRatio() { localStorage.removeItem(overviewRatioKey) }
export function useDisplayPreferences(ready: boolean, generation: number) {
  const [overviewRatio, setOverviewRatio] = useState(readOverviewRatio)
  const [value, setValue] = useState<DisplayPreferences>(defaults)
  const [loaded, setLoaded] = useState(!realFilmsAvailable)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [retry, setRetry] = useState(0)
  const guard = useRef(false)
  const revision = useRef(0)
  useEffect(() => {
    setOverviewRatio(readOverviewRatio())
    const current = ++revision.current
    guard.current = false; setBusy(false); setError(''); setValue(defaults); setLoaded(!realFilmsAvailable)
    if (!realFilmsAvailable || !ready) return
    void invoke<DisplayPreferences>('load_display_preferences').then(saved => {
      if (revision.current === current) { setValue(saved); setLoaded(true) }
    }).catch(() => { if (revision.current === current) setError('显示比例暂时无法读取，请重试。') })
    return () => { revision.current++ }
  }, [ready, generation, retry])
  const save = async (command: string, args: Record<string, unknown>, apply: (old: DisplayPreferences) => DisplayPreferences) => {
    if (!loaded || guard.current) return
    const current = revision.current
    guard.current = true; setBusy(true); setError('')
    try {
      if (realFilmsAvailable) await invoke(command, args)
      if (current === revision.current) setValue(apply)
    } catch { if (current === revision.current) setError('显示比例未保存，已保留原比例，请重试。') }
    finally { if (current === revision.current) { guard.current = false; setBusy(false) } }
  }
  return { value, loaded, busy, error, retry: useCallback(() => setRetry(n => n + 1), []),
    overviewRatio,
    setOverview: (ratio: FilmRatio) => {
      try { localStorage.setItem(overviewRatioKey, ratio); setOverviewRatio(ratio); setError('') }
      catch { setError('显示比例未保存，已保留原比例，请重试。') }
    },
    setFilm: (id: string, ratio: FilmRatio) => save('save_film_cover_ratios', { libraryIds: [id], ratio }, old => ({ ...old, libraryRatios: { ...old.libraryRatios, [id]: ratio } })),
    setActor: (ratio: ActorRatio) => save('save_actor_photo_ratio', { ratio }, old => ({ ...old, actorRatio: ratio })),
  }
}
export type DisplayController = ReturnType<typeof useDisplayPreferences>
