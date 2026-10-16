import { useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { mockIPC } from '@tauri-apps/api/mocks'
import { emit } from '@tauri-apps/api/event'
import '../../src/styles.css'
import '../../src/settings.css'

const actions: string[] = []
Object.assign(window, { captionTestActions: actions, isTauri: true, captionTestClose: () => emit('window-close-confirm', 'tray_unavailable') })
mockIPC((command, payload) => {
  if (command === 'window_titlebar_action') { actions.push(String(payload?.action)); return false }
  if (command === 'load_ignored_directory_names') return []
  if (command === 'list_data_backups') return [{ id: 'caption-synthetic-backup', created_at_unix_ms: 1700000000000 }]
  if (command === 'load_application_options') return { closeWindowExits: false, monitorLibraries: true }
  if (command === 'resolve_window_close') { actions.push(`resolve-${payload?.action}`); return }
  return []
}, { shouldMockEvents: true })
const { WindowTitleBar } = await import('../../src/components/WindowTitleBar')
const { IgnoredDirectoryNames } = await import('../../src/components/IgnoredDirectoryNames')
const { BackupSection } = await import('../../src/components/BackupSection')
const { ApplicationOptions } = await import('../../src/components/ApplicationOptions')
const { WindowCloseDialog } = await import('../../src/components/WindowCloseDialog')

function Fixture() {
  const first = useRef<HTMLDialogElement>(null)
  const second = useRef<HTMLDialogElement>(null)
  const [removed, setRemoved] = useState(false)
  return <div className="desktop-window">
    <WindowTitleBar />
    <WindowCloseDialog />
    <main style={{ paddingTop: 60 }}>
      <button id="open-first" onClick={() => first.current?.showModal()}>打开确认</button>
      <button id="background">背景操作</button>
      <IgnoredDirectoryNames disabled={false} />
      <BackupSection />
      <ApplicationOptions />
      <dialog id="second" ref={second} className="settings-restore-dialog">
        <h3>第二个确认</h3><button autoFocus onClick={() => second.current?.close()}>返回上一层</button>
      </dialog>
      {!removed && <dialog id="first" ref={first} className="settings-restore-dialog">
        <h3>第一个确认</h3>
        <button id="open-second" autoFocus onClick={() => second.current?.showModal()}>打开第二层</button>
        <button id="remove-first" onClick={() => setRemoved(true)}>卸载确认</button>
        <button onClick={() => first.current?.close()}>取消确认</button>
      </dialog>}
    </main>
  </div>
}
createRoot(document.getElementById('root')!).render(<Fixture />)
