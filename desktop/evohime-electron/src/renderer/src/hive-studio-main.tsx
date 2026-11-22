/**
 * Hive Studio — точка монтирования dev-стенда (`src/renderer/hive-studio.html`).
 *
 * Запускается только dev-сервером `npm run studio` (vite.studio.config.ts) и
 * открывается в обычном браузере. Не входит в production-бандл: rollup input
 * в `electron.vite.config.ts` перечисляет только `index.html`.
 */

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { StudioApp } from './hive-studio-app'

const container = document.getElementById('root')
if (!container) {
  throw new Error('hive studio root element is missing')
}

createRoot(container).render(
  <StrictMode>
    <StudioApp />
  </StrictMode>
)
