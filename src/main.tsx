import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import { EmbeddedPlayerHost } from './components/EmbeddedPlayer'
import { WindowTitleBar } from './components/WindowTitleBar'
import { DiagnosticBoundary } from './components/DiagnosticBoundary'
import { installFrontendDiagnostics } from './data/diagnostics'
import { InterfaceLanguageSync } from './components/InterfaceLanguage'

const disposeDiagnostics = installFrontendDiagnostics()
if (import.meta.hot) import.meta.hot.dispose(disposeDiagnostics)
ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode><InterfaceLanguageSync /><DiagnosticBoundary>{new URLSearchParams(location.search).get('player') === '1' ? <><WindowTitleBar player /><EmbeddedPlayerHost standalone /></> : <App />}</DiagnosticBoundary></React.StrictMode>)
