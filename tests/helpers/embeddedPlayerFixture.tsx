import { useState } from 'react'
import { createRoot } from 'react-dom/client'
import { invoke } from '@tauri-apps/api/core'
import { EmbeddedPlayerHost } from '../../src/components/EmbeddedPlayer'
import { WindowTitleBar } from '../../src/components/WindowTitleBar'
import { launchFilmVideo } from '../../src/data/playback'
import '../../src/styles.css'
import '../../src/app-motion.css'

function Fixture() {
  const [error, setError] = useState('')
  const [reduce, setReduce] = useState(false)
  const play = async (index: number) => {
    try {
      const rows = await invoke<{ id: string; videos: { id: string }[] }[]>('probe_catalog')
      await launchFilmVideo(rows[0].id, rows[0].videos[index].id)
      window.dispatchEvent(new CustomEvent('probe-launch-complete', { detail: performance.now() }))
      setError('')
    } catch (error) { setError(String(error)) }
  }
  return <><WindowTitleBar /><dialog id="probe-confirm"><p>退出确认</p><button onClick={event => event.currentTarget.closest('dialog')?.close()}>取消退出</button></dialog><main style={{ padding: '70px 40px', minHeight: '150vh' }}>
    <h1>合成片库 · 播放器验证</h1><p>当前筛选：已收藏；滚动位置保留。</p>
    <button onClick={() => { void play(0) }}>播放第一集</button><button onClick={() => { void play(1) }}>播放第二集</button>
    <button onClick={() => { void invoke('probe_display', { display: 'overlay' }) }}>使用覆盖模式</button><button onClick={() => { void invoke('probe_display', { display: 'window' }) }}>使用独立窗口</button>
    <label><input type="checkbox" checked={reduce} onChange={event => setReduce(event.target.checked)} />减少动态效果</label><p role="alert">{error}</p>
  </main><EmbeddedPlayerHost reduceMotion={reduce} /></>
}
createRoot(document.getElementById('root')!).render(<Fixture />)
