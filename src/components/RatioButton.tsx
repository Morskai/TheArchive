import { t, useLocale } from '../i18n/react'
interface Props { label: string; value: string; next: string; disabled?: boolean; onClick: () => void }
export function RatioButton({ label, value, next, disabled, onClick }: Props) {
  useLocale()
  return <div className="card-ratio-control"><span>{t("卡片比例")}</span><button type="button" className="ratio-button" aria-label={t(disabled ? "{0}，当前 {1}" : "{0}，当前 {1}，点击切换为 {2}", disabled ? [label, value] : [label, value, next])} disabled={disabled} onClick={onClick}>{value}</button></div>
}
