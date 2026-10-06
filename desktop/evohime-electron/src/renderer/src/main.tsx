import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { App } from './App'
import { applyAppearance, loadAppearance } from './appearance'
import { setAppLocale } from './i18n'
import './styles.css'

const appearance = loadAppearance()
applyAppearance(appearance)
setAppLocale(appearance.locale)

const container = document.getElementById('root')
if (!container) {
  throw new Error('renderer root element is missing')
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>
)
