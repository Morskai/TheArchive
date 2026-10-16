type PressTarget = Pick<HTMLElement, 'animate' | 'classList'>

/** At most eight short feedback sequences; rapid repeat replaces the old sequence. */
export function createPressFeedback() {
  const running = new Map<PressTarget, { stop: () => void }>()
  return {
    press(target: PressTarget, { motion = true, preserveFocus = false } = {}) {
      running.get(target)?.stop()
      if (running.size >= 8) running.values().next().value?.stop()
      const animations: Animation[] = []
      let timer: ReturnType<typeof setTimeout> | undefined
      let disposed = false
      let remaining = 0
      const release = (cancel: boolean) => {
        if (disposed) return
        disposed = true
        if (timer !== undefined) clearTimeout(timer)
        if (running.get(target) === entry) {
          running.delete(target)
          target.classList.remove('button-acknowledged')
        }
        if (cancel) animations.forEach(animation => animation.cancel())
      }
      const entry = { stop: () => release(true) }
      running.set(target, entry)
      if (!preserveFocus) target.classList.add('button-acknowledged')
      if (motion) {
        try {
          animations.push(target.animate([{ scale: '.965' }, { scale: '1' }], {
            duration: 160, easing: 'cubic-bezier(.16,.8,.2,1)',
          }))
          if (!preserveFocus) animations.push(target.animate([
            { filter: 'brightness(1.12)' },
            { filter: 'brightness(1)' },
          ], { duration: 240, easing: 'ease-out' }))
        } catch { /* Keep a brief static acknowledgment if animation is unavailable. */ }
      }
      remaining = animations.length
      for (const animation of animations) {
        animation.onfinish = () => { if (--remaining === 0 && timer === undefined) release(false) }
        animation.oncancel = entry.stop
      }
      if (!motion || !animations.length || (!preserveFocus && animations.length === 1)) {
        timer = setTimeout(() => release(true), 240)
      }
    },
    clear() {
      for (const entry of [...running.values()]) entry.stop()
    },
  }
}
