import { describe, expect, it } from 'vitest'

import { overlayVisible } from '../src/main/overlay'

/**
 * Индикатор «Ева слушает» поверх всех окон.
 *
 * Видимость — чистая функция состояния слушания, как `trayIconName` для
 * трея: индикатор виден ровно тогда, когда открыт микрофон, и не подменяет
 * «неизвестно» на «выключено».
 */
describe('видимость индикатора поверх окон', () => {
  it('виден, пока микрофон открыт', () => {
    expect(overlayVisible('listening')).toBe(true)
    expect(overlayVisible('starting')).toBe(true)
  })

  it('скрыт во всех остальных состояниях, включая неизвестное', () => {
    expect(overlayVisible('paused_by_user')).toBe(false)
    expect(overlayVisible('stopped')).toBe(false)
    expect(overlayVisible('device_disconnected')).toBe(false)
    expect(overlayVisible(null)).toBe(false)
  })
})
