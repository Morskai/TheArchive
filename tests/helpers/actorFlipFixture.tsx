import { useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { ActorFlip } from '../../src/components/ActorFlip'
import { demoActors } from '../../src/data/actors'
import '../../src/styles.css'
import '../../src/app-motion.css'

function Fixture() {
  const [count, setCount] = useState(8)
  const [reduce, setReduce] = useState(false)
  const [active, setActive] = useState(true)
  const [status, setStatus] = useState('ready')
  const [opened, setOpened] = useState('')
  const session = useRef({ played: false })
  return <main style={{ padding: 24 }}>
    <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
      <label>人数 <select aria-label="人数" value={count} onChange={event => setCount(Number(event.target.value))}>{[0, 1, 5, 7, 8].map(value => <option key={value}>{value}</option>)}</select></label>
      <label><input type="checkbox" checked={reduce} onChange={event => setReduce(event.target.checked)} />减少动态效果</label>
      <button onClick={() => setActive(value => !value)}>切页</button>
      <select aria-label="状态" value={status} onChange={event => setStatus(event.target.value)}>{['ready', 'loading', 'error'].map(value => <option key={value}>{value}</option>)}</select>
      <output id="opened">{opened}</output>
    </div>
    <div hidden={!active}><ActorFlip active={active} reduceMotion={reduce} entranceSession={session.current}
      actors={demoActors.slice(0, count).map((actor, index) => index ? actor : { ...actor, name: '很长的演员姓名用于检查狭窄卡片与中英文 Mixed Name' })} films={[]}
      loading={status === 'loading'} error={status === 'error' ? '合成读取失败' : undefined} onOpenActor={setOpened} onOpenActorLibrary={() => setOpened('library')} /></div>
  </main>
}
createRoot(document.getElementById('root')!).render(<Fixture />)
