import { t, useLocale } from '../i18n/react'
import { ActorManagementDialog } from './ActorManagementDialog'

export function ActorAdd({ onManual, onScan, onClose }: { onManual: () => void; onScan: () => void; onClose: () => void }) {
  useLocale()
  return <ActorManagementDialog title={t("新增演员")} busy={false} onClose={onClose}>
    <div className="actor-add-options">
      <button type="button" onClick={onManual}><svg viewBox="0 0 32 32" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><circle cx="13" cy="10" r="5" /><path d="M4 27v-3a9 9 0 0 1 14-7M25 18v10M20 23h10" /></svg><span>{t("手动添加")}</span></button>
      <button type="button" onClick={onScan}><svg viewBox="0 0 32 32" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M3 11V7a2 2 0 0 1 2-2h7l3 4h12a2 2 0 0 1 2 2v4M5 27h11M3 15h14l-2 12H5a2 2 0 0 1-2-2V15Z" /><circle cx="23" cy="21" r="5" /><path d="m27 25 3 3" /></svg><span>{t("扫描添加")}</span></button>
    </div>
  </ActorManagementDialog>
}
