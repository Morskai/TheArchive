import { t, useLocale } from '../i18n/react'
import { useEffect, useId, useState, type CSSProperties, type ReactNode } from 'react'
import filmEmblem from '../../src-tauri/icons/film-empty.png'
import actorEmblem from '../assets/actor-library-emblem.png'
import cardBack from '../assets/flip-card-back.png'
import '../library-empty-state.css'

type LibraryKind = 'films' | 'actors' | 'flip'

function FilmMark() {
  useLocale()
  const mask = useId()
  return <span className="library-film-mark"><img src={filmEmblem} alt="" draggable={false} />
    <svg className="library-icon-motion" viewBox="0 0 256 256" aria-hidden="true" focusable="false">
      <defs><mask id={mask}><rect x="31" y="28" width="194" height="193" rx="10" fill="white" /><path d="M99 91Q99 81 109 87L162 118Q173 125 162 132L109 164Q99 170 99 158Z" fill="black" />{[40,78,116,154,192].flatMap(y => [32,200].map(x => <rect key={`${x}-${y}`} x={x} y={y} width="26" height="28" rx="5" fill="black" />))}</mask></defs>
      <g mask={`url(#${mask})`}>
        <rect className="library-icon-background" width="256" height="256" />
        {Array.from({ length: 36 }, (_, i) => <circle key={i} className="library-icon-grain" cx={67 + (i * 37 % 126)} cy={32 + (i * 23 % 181)} r={i % 4 === 0 ? 1.5 : 1} style={{ '--particle-delay': `${-(i % 12) * .19}s` } as CSSProperties} />)}
        {Array.from({ length: 8 }, (_, i) => <path key={i} className={`library-icon-star${i % 3 === 0 ? ' is-brighter' : ''}`} d={`M${70 + (i * 41 % 118) - 4} ${39 + (i * 29 % 172)}h8M${70 + (i * 41 % 118)} ${39 + (i * 29 % 172) - 4}v8`} style={{ '--particle-delay': `${-i * .1125}s` } as CSSProperties} />)}
      </g>
    </svg>
  </span>
}

function LibraryMark({ kind }: { kind: LibraryKind }) {
  useLocale()
  if (kind === 'flip') return <div className="empty-card-table" aria-hidden="true">
    {Array.from({ length: 8 }, (_, index) => <img key={index} src={cardBack} alt="" draggable={false} />)}
  </div>
  return kind === 'films' ? <FilmMark /> : <img src={actorEmblem} alt="" draggable={false} />
}

export function LibraryEmptyState({ kind, title, onMarkClick, maxCardWidth, children }: {
  kind: LibraryKind
  title: string
  onMarkClick?: () => void
  maxCardWidth?: number
  children?: ReactNode
}) {
  useLocale()
  const [hovered, setHovered] = useState(false)
  useEffect(() => {
    if (!hovered) return
    const stop = () => setHovered(false)
    const visibility = () => { if (document.hidden) stop() }
    window.addEventListener('blur', stop)
    document.addEventListener('visibilitychange', visibility)
    return () => { window.removeEventListener('blur', stop); document.removeEventListener('visibilitychange', visibility) }
  }, [hovered])
  return <div className={`library-empty-state library-empty-${kind}`} style={maxCardWidth === undefined ? undefined : { '--empty-card-limit': `${maxCardWidth}px` } as CSSProperties}>
    {kind === 'flip' ? <LibraryMark kind={kind} /> : onMarkClick ? <button className={`library-empty-mark${hovered ? ' is-hovered' : ''}`} type="button" onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)} onClick={() => { setHovered(false); onMarkClick() }} aria-label={t("前往设置扫描影片")} title={t("前往设置扫描影片")}><LibraryMark kind={kind} /></button>
      : <div className="library-empty-mark"><LibraryMark kind={kind} /></div>}
    <h2>{title}</h2>
    {children}
  </div>
}
