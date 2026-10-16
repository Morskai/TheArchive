import { useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { AppSelect } from '../../src/components/AppSelect'
import { ConfigurationDirectory } from '../../src/components/ConfigurationDirectory'
import { useButtonPress } from '../../src/components/useAppMotion'
import '../../src/styles.css'
import '../../src/settings.css'
import '../../src/app-motion.css'

function Fixture() {
  const root = useRef<HTMLDivElement>(null)
  const [value, setValue] = useState('cover')
  const [reduced, setReduced] = useState(false)
  useButtonPress(root, !reduced)
  return <div ref={root} style={{ padding: 24 }}>
    <header className="topbar"><ConfigurationDirectory desktop path="Z:\FICTIONAL_ONLY\software" /></header>
    <label style={{ display: 'flex', gap: 12, marginTop: 24 }}>显示方式<AppSelect aria-label="图片显示方式" value={value} onChange={event => setValue(event.target.value)}><option value="cover">铺满裁切</option><option value="contain">完整显示</option></AppSelect></label>
    <button id="action" className="settings-primary" style={{ marginTop: 24 }}>普通操作</button>
    <button id="reduce" className="settings-action" onClick={() => setReduced(!reduced)}>减少动态效果</button>
  </div>
}
createRoot(document.getElementById('root')!).render(<Fixture />)
