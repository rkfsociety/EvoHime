import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { UpdaterApp } from './UpdaterApp'
import { loadAppearance } from './appearance'
import { setAppLocale } from './i18n'

const storedAppearance = window.localStorage.getItem('evohime.appearance.v1')
const systemLocale = navigator.language.toLowerCase().startsWith('en') ? 'en' : 'ru'
setAppLocale(storedAppearance ? loadAppearance().locale : systemLocale)

const container = document.getElementById('root')
if (!container) throw new Error('updater renderer root element is missing')

document.getElementById('boot-fallback')?.remove()

createRoot(container).render(
  <StrictMode>
    <UpdaterApp />
  </StrictMode>
)
