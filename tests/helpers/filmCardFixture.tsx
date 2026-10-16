import { useState } from 'react'
import { createRoot } from 'react-dom/client'
import { FilmCard, type FilmCardView } from '../../src/components/FilmCard'
import '../../src/styles.css'
import '../../src/film-library.css'

const items: Array<FilmCardView & { source: string }> = [
  { id: 'numbered', code: 'ABC-123', title: '海岸的午后', source: '精选收藏', rating: 4.5, favorite: true, tags: ['中文字幕'], hasCover: true, artwork: 1 },
  { id: 'short', code: '', title: '星际穿越', source: '电影', rating: 5, favorite: true, tags: ['中文字幕'], hasCover: true, artwork: 3 },
  { id: 'long', code: '   ', title: '一段很长的电影片名用于核对两行省略以及普通影视收藏中的完整标题查看方式', source: '经典电影与世界各地珍藏版本合集非常长的片库名称', rating: null, favorite: false, tags: [], hasCover: true, artwork: 7 },
  { id: 'pending', code: '编号待核对', title: '编号资料尚待确认', source: '待整理', rating: 3, favorite: false, tags: ['中文字幕'], hasCover: true, artwork: 8 },
  { id: 'numbered-source', code: 'XYZ-456', title: '雨夜', source: '一个非常长的来源片库名称用于窄卡片布局验证', rating: 4, favorite: false, tags: [], hasCover: true, artwork: 4 },
  { id: 'empty-source', code: '', title: 'Arrival', source: '', rating: null, favorite: false, tags: [], hasCover: false, artwork: 0 },
]
const actions: string[] = []
Object.assign(window, { filmCardActions: actions })
function Fixture() {
  const [editing, setEditing] = useState(false)
  const [selected, setSelected] = useState<string[]>([])
  return <main className="library-section" style={{ padding: '28px 24px', maxWidth: 1280, margin: 'auto' }}>
    <div className="section-heading"><div><h1>影片库</h1><p>合成资料 · 实际 FilmCard 与卡片样式</p></div><button id="manage" onClick={() => { setEditing(!editing); setSelected([]) }}>{editing ? '退出影片管理' : '影片管理'}</button></div>
    <div className="film-grid density-standard" id="cards">{items.map(film => <FilmCard key={film.id} film={film} sourceName={film.source} codePlaceholder={film.code === '编号待核对'} editing={editing} selected={selected.includes(film.id)} onOpen={() => {
      actions.push(`${editing ? 'select' : 'open'}:${film.id}`)
      if (editing) setSelected(ids => ids.includes(film.id) ? ids.filter(id => id !== film.id) : [...ids, film.id])
    }} onPlay={() => actions.push(`play:${film.id}`)} />)}</div>
  </main>
}
createRoot(document.getElementById('root')!).render(<Fixture />)
