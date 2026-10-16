import { useEffect, useState } from 'react'

// A native modal makes the rest of the document inert, regardless of z-index.
// Keep the caption inside the last-opened modal so OS window actions stay usable.
export function useModalCaptionHost(enabled: boolean) {
  const [host, setHost] = useState<HTMLDialogElement | null>(null)
  useEffect(() => {
    if (!enabled) return
    let stack = [...document.querySelectorAll<HTMLDialogElement>('dialog:modal')]
    const add = (dialog: HTMLDialogElement) => {
      stack = stack.filter(item => item !== dialog)
      if (dialog.matches(':modal')) stack.push(dialog)
    }
    const refresh = () => {
      stack = stack.filter(dialog => dialog.isConnected && dialog.matches(':modal'))
      setHost(stack.at(-1) ?? null)
    }
    refresh()
    const observer = new MutationObserver(records => {
      for (const record of records) {
        if (record.type === 'attributes' && record.target instanceof HTMLDialogElement) add(record.target)
        for (const node of record.addedNodes) {
          if (!(node instanceof Element)) continue
          if (node instanceof HTMLDialogElement) add(node)
          node.querySelectorAll<HTMLDialogElement>('dialog:modal').forEach(add)
        }
      }
      refresh()
    })
    observer.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ['open'] })
    return () => observer.disconnect()
  }, [enabled])
  return host
}
