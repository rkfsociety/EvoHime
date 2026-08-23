import { BrowserWindow, screen } from 'electron'

import type { ListeningState } from '@shared/api'

import { resourcePath } from './paths'

/**
 * Индикатор «Ева слушает» поверх всех окон (этап после 04.5).
 *
 * Отдельное маленькое окно в углу экрана, а не иконка в трее: трей не виден,
 * когда пользователь занят другим приложением на весь экран, а этот индикатор
 * обязан быть виден всегда, пока микрофон открыт. Своего состояния у окна
 * нет: оно рисует то же `ambient.state`, что и трей с панелью, и коротко
 * вспыхивает, когда ядро распознало обращение «Ева, …» (`ambient.voice_command`).
 *
 * Окно не перехватывает клики (`setIgnoreMouseEvents`) и не попадает в
 * Alt+Tab и панель задач: это индикатор, а не поверхность управления.
 */

const OVERLAY_SIZE = 72
const OVERLAY_MARGIN = 24
/** Как долго держится «вспышка» после услышанного обращения. */
const HEARD_FLASH_MS = 2_500

export interface OverlayController {
  /** Перерисовывает индикатор по состоянию, пришедшему от ядра. */
  setListeningState(state: ListeningState | null): void
  /** Короткая вспышка: ядро распознало «Ева, …» в услышанном. */
  flashHeard(): void
  destroy(): void
}

/** Видимость индикатора по состоянию слушания: виден, только пока микрофон открыт. */
export function overlayVisible(state: ListeningState | null): boolean {
  return state === 'listening' || state === 'starting'
}

function overlayHtml(): string {
  const iconUrl = `file://${resourcePath('evohime-agent-listening.ico').replace(/\\/g, '/')}`
  return `<!doctype html>
<html>
<head>
<meta charset="utf-8">
<style>
  html, body { margin: 0; padding: 0; background: transparent; overflow: hidden; }
  .badge {
    width: ${OVERLAY_SIZE}px;
    height: ${OVERLAY_SIZE}px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(16, 16, 20, 0.55);
    box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.08);
    animation: pulse 2.4s ease-in-out infinite;
  }
  .badge img { width: 40px; height: 40px; }
  .badge.heard {
    background: rgba(64, 200, 120, 0.75);
    box-shadow: 0 0 18px 4px rgba(64, 200, 120, 0.65);
    animation: none;
  }
  @keyframes pulse {
    0%, 100% { opacity: 0.55; transform: scale(1); }
    50% { opacity: 0.9; transform: scale(1.06); }
  }
</style>
</head>
<body>
  <div class="badge" id="badge"><img src="${iconUrl}" alt=""></div>
  <script>
    window.__setOverlayState = function (state) {
      document.getElementById('badge').className = 'badge' + (state === 'heard' ? ' heard' : '')
    }
  </script>
</body>
</html>`
}

export function createOverlay(): OverlayController {
  const display = screen.getPrimaryDisplay()
  const { width, height } = display.workArea
  const window = new BrowserWindow({
    width: OVERLAY_SIZE,
    height: OVERLAY_SIZE,
    x: display.workArea.x + width - OVERLAY_SIZE - OVERLAY_MARGIN,
    y: display.workArea.y + height - OVERLAY_SIZE - OVERLAY_MARGIN,
    frame: false,
    transparent: true,
    hasShadow: false,
    resizable: false,
    movable: false,
    minimizable: false,
    maximizable: false,
    closable: false,
    focusable: false,
    skipTaskbar: true,
    show: false,
    webPreferences: {
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      devTools: false
    }
  })

  window.setAlwaysOnTop(true, 'screen-saver')
  window.setIgnoreMouseEvents(true, { forward: true })
  window.setMenuBarVisibility(false)

  void window.loadURL(`data:text/html;charset=utf-8,${encodeURIComponent(overlayHtml())}`)

  let visible = false
  let flashTimer: NodeJS.Timeout | null = null

  const setPageState = (state: 'idle' | 'heard'): void => {
    if (window.isDestroyed()) return
    void window.webContents.executeJavaScript(`window.__setOverlayState?.(${JSON.stringify(state)})`)
  }

  return {
    setListeningState: (state) => {
      const next = overlayVisible(state)
      if (next === visible) return
      visible = next
      if (window.isDestroyed()) return
      if (visible) {
        setPageState('idle')
        window.showInactive()
      } else {
        if (flashTimer) {
          clearTimeout(flashTimer)
          flashTimer = null
        }
        window.hide()
      }
    },
    flashHeard: () => {
      if (!visible || window.isDestroyed()) return
      setPageState('heard')
      if (flashTimer) clearTimeout(flashTimer)
      flashTimer = setTimeout(() => {
        flashTimer = null
        setPageState('idle')
      }, HEARD_FLASH_MS)
      flashTimer.unref?.()
    },
    destroy: () => {
      if (flashTimer) clearTimeout(flashTimer)
      if (!window.isDestroyed()) window.destroy()
    }
  }
}
