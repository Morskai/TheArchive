/** Native snapshots keep the same portrait continuous without copying or reloading image data. */
export function createActorPortraitTransition() {
  let cancelCurrent: ((completeUpdate?: boolean) => void) | undefined
  const cancel = (completeUpdate = false) => { cancelCurrent?.(completeUpdate); cancelCurrent = undefined }
  return {
    cancel,
    start(fromSelector: string, toSelector: string, update: () => void) {
      cancel()
      const from = document.querySelector<HTMLElement>(fromSelector)
      if (!document.startViewTransition || !from || !from.getBoundingClientRect().width) { update(); return }
      let to: HTMLElement | null = null
      let cancelled = false
      let completeOnCancel = false
      let updated = false
      const previousFrom = from.style.viewTransitionName
      let previousTo = ''
      const cleanup = () => {
        from.style.viewTransitionName = previousFrom
        if (to) to.style.viewTransitionName = previousTo
        if (cancelCurrent === stop) { delete document.documentElement.dataset.portraitTravel; cancelCurrent = undefined }
      }
      let transition: ViewTransition | undefined
      const stop = (completeUpdate = false) => { cancelled = true; completeOnCancel = completeUpdate; transition?.skipTransition(); cleanup() }
      cancelCurrent = stop
      from.style.viewTransitionName = 'actor-portrait-travel'
      document.documentElement.dataset.portraitTravel = 'true'
      try {
        transition = document.startViewTransition(() => {
          if (cancelled) { if (completeOnCancel && !updated) { update(); updated = true }; return }
          update(); updated = true
          to = document.querySelector<HTMLElement>(toSelector)
          if (to && to.getBoundingClientRect().width) {
            previousTo = to.style.viewTransitionName
            to.style.viewTransitionName = 'actor-portrait-travel'
          } else transition?.skipTransition()
        })
        void transition.ready.catch(() => {})
        void transition.finished.then(cleanup, cleanup)
      } catch {
        cleanup()
        if (!updated) update()
      }
    },
  }
}
