import { t, useLocale } from '../i18n/react'
import { useEffect, useState } from 'react'
import '../scroll-to-top.css'

export function ScrollToTopButton() {
  useLocale()
  const [visible, setVisible] = useState(false)
  useEffect(() => {
    const update = () => setVisible(window.scrollY > 300)
    update()
    window.addEventListener('scroll', update, { passive: true })
    return () => window.removeEventListener('scroll', update)
  }, [])
  return visible ? <button className="scroll-to-top" type="button" aria-label={t("回到顶部")} title={t("回到顶部")} onClick={() => {
    const reduceMotion = document.documentElement.dataset.reduceMotion === 'true' || window.matchMedia('(prefers-reduced-motion: reduce)').matches
    window.scrollTo({ top: 0, behavior: reduceMotion ? 'instant' : 'smooth' })
  }}><span aria-hidden="true">↑</span> {t("置顶")}</button> : null
}
