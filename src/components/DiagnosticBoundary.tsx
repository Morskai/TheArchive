import { t, useLocale } from '../i18n/react'
import { Component, type ErrorInfo, type ReactNode } from 'react'
import { reportDiagnostic } from '../data/diagnostics'
import { exceptionEvent } from '../data/diagnosticEvents'
import { DiagnosticSection } from './DiagnosticSection'

export class DiagnosticBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  componentDidCatch(error: Error, _info: ErrorInfo) { reportDiagnostic(exceptionEvent('render_error', error)) }
  render() {
    if (this.state.failed) return <DiagnosticFallback />
    return this.props.children
  }
}

function DiagnosticFallback() {
  useLocale()
  return <main className="diagnostic-fallback" role="alert"><h1>{t("界面暂时无法显示")}</h1><p>{t("请重新打开界面后重试；若仍有问题，可导出诊断日志交给开发者。")}</p><button type="button" onClick={() => window.location.reload()}>{t("重新打开界面")}</button><div className="diagnostic-fallback-tools"><DiagnosticSection /></div></main>
}
