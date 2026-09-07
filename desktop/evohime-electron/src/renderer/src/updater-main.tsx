import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { UpdaterApp } from './UpdaterApp'

const container = document.getElementById('root')
if (!container) throw new Error('updater renderer root element is missing')

createRoot(container).render(
  <StrictMode>
    <UpdaterApp />
  </StrictMode>
)
