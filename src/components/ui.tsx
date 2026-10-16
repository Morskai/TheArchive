import { t, useLocale } from '../i18n/react'
import type { CSSProperties, ReactNode } from 'react'
import archiveMark from '../assets/archive-logo-gold.png'

export function PageFrame({ children }: { children: ReactNode }) {
  return <main className="workspace">{children}</main>
}

export function ArrowIcon({ direction }: { direction: 'up' | 'down' | 'left' }) {
  return <svg className={`arrow-icon arrow-${direction}`} width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false"><path d={direction === 'left' ? 'M15 9H3m5-5-5 5 5 5' : 'M9 15V3m-5 5 5-5 5 5'} /></svg>
}

const navigation = ['影片库', '演员库', '翻牌', '播放历史', '问题中心', '设置']
export type Page = 'films' | 'actors' | 'actorFlip' | 'history' | 'issues' | 'settings'
const pageKeys: Page[] = ['films', 'actors', 'actorFlip', 'history', 'issues', 'settings']

function NavIcon({ index }: { index: number }) {
  const shared = { fill: 'none', stroke: 'currentColor', strokeWidth: 1.45, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const }
  return <svg viewBox="0 0 28 28" aria-hidden="true" focusable="false" {...shared}>
    {index === 0 && <><rect x="5.5" y="6.5" width="17" height="15" rx="1.8" /><path d="M9.5 6.5v15M18.5 6.5v15M5.5 10h4M5.5 18h4M18.5 10h4M18.5 18h4" /></>}
    {index === 1 && <><circle cx="14" cy="10" r="3.1" /><path d="M7 21c.6-4 3.2-6.3 7-6.3s6.4 2.3 7 6.3M9.5 21h9" /></>}
    {index === 2 && <><rect x="5.5" y="7" width="13.5" height="17" rx="1.6" /><path d="M9 4.5h12.8c.9 0 1.7.8 1.7 1.7V20M12.2 15.2l1.8-3.6 1.8 3.6 3.6 1.8-3.6 1.8-1.8 3.6-1.8-3.6-3.6-1.8z" /></>}
    {index === 3 && <><circle cx="14" cy="14" r="9" /><path d="M14 8.5V14l3.5 2.4M7.6 7.6l-2.1-1.8" /></>}
    {index === 4 && <><path d="M14 4.8 23.2 14 14 23.2 4.8 14 14 4.8Z" /><path d="M14 9.4v6.4" /><circle cx="14" cy="19" r=".8" fill="currentColor" stroke="none" /></>}
    {index === 5 && <><path d="M5.5 8.5h17M5.5 14h17M5.5 19.5h17" /><circle cx="11" cy="8.5" r="2.2" fill="#151311" /><circle cx="18" cy="14" r="2.2" fill="#151311" /><circle cx="12.5" cy="19.5" r="2.2" fill="#151311" /></>}
  </svg>
}

export function Navigation({ activePage, onNavigate, collapsed, onToggle }: { activePage: Page; onNavigate: (page: Page) => void; collapsed: boolean; onToggle: () => void }) {
  useLocale()
  return <aside className="sidebar">
    <button type="button" className="navigation-toggle" aria-label={collapsed ? t("展开导航栏") : t("收起导航栏")} title={collapsed ? t("展开导航栏") : t("收起导航栏")} aria-expanded={!collapsed} aria-controls="primary-navigation" onClick={onToggle}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M9 4v16" /><path d={collapsed ? 'm13 9 3 3-3 3' : 'm16 9-3 3 3 3'} /></svg></button>
    <div className="brand"><img className="brand-mark" src={archiveMark} alt="" draggable={false} /><div className="brand-name">THE ARCHIVE</div><div className="brand-sub">{t("私人影像收藏室")}</div></div>
    <div className="nav-label">NAVIGATION BAR</div>
    <nav className="nav" id="primary-navigation" aria-label={t("主导航")} style={{ '--nav-index': pageKeys.indexOf(activePage) } as CSSProperties}>
      <span className="nav-indicator" aria-hidden="true" />
      {navigation.map((item, index) => <button key={item} type="button" aria-label={t(item)} title={collapsed ? t(item) : undefined} onClick={() => onNavigate(pageKeys[index])} className={`nav-item ${activePage === pageKeys[index] ? 'active' : ''}`} aria-current={activePage === pageKeys[index] ? 'page' : undefined}>
        <span className="nav-symbol"><NavIcon index={index} /></span><span className="nav-text">{t(item)}</span>
      </button>)}
    </nav>
    <span className="navigation-version" aria-label={t("版本 v1.0")}>v1.0</span>
  </aside>
}

export function FilterChip({ active, children, onClick }: { active: boolean; children: ReactNode; onClick: () => void }) {
  return <button className={`filter-chip ${active ? 'selected' : ''}`} type="button" aria-pressed={active} onClick={onClick}>{children}</button>
}

export function EmptyState({ onReset, title = t("没有符合条件的影片") }: { onReset: () => void; title?: string }) {
  useLocale()
  return <div className="empty-state" role="status"><div className="empty-orbit" aria-hidden="true"><span>✦</span></div><h3>{title}</h3><p>{t("试试更换关键词，或清除当前筛选。")}</p><button className="button button-gold" type="button" onClick={onReset}>{t("清除筛选")}</button></div>
}
