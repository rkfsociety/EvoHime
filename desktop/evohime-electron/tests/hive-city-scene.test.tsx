// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import {
  HiveCityScene,
  MAX_CAMERA_ZOOM,
  MIN_CAMERA_ZOOM
} from '../src/renderer/src/components/hive'
import { buildHoneycombStudioWorld } from '../src/renderer/src/hive-studio-simulation'

describe('HiveCityScene (2D Canvas)', () => {
  afterEach(() => {
    cleanup()
  })

  it('renders 2D top-down canvas in fallback mode when world is omitted', () => {
    const { rerender } = render(<HiveCityScene status="observing" />)
    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('data-mode')).toBe('2d-canvas')
    expect(scene.getAttribute('data-status')).toBe('observing')
    expect(screen.getByTestId('hive-city-canvas')).toBeTruthy()

    rerender(<HiveCityScene status="active" />)
    expect(screen.getByTestId('hive-city-scene').getAttribute('data-status')).toBe('active')
  })

  it('clamps zoom within [MIN_CAMERA_ZOOM, MAX_CAMERA_ZOOM] and supports MMB pan', () => {
    const world = buildHoneycombStudioWorld({
      stage: 'pipeline',
      paused: false,
      evaSourceConfigured: true,
      projectCount: 2,
      agentCount: 4,
      exitsOverride: 'auto',
      remodelSeed: 0
    })

    render(<HiveCityScene status="active" world={world} queenWorking />)
    const canvas = screen.getByTestId('hive-city-canvas')
    const zoomLabel = screen.getByTestId('hive-camera-zoom')

    for (let i = 0; i < 20; i += 1) {
      fireEvent.wheel(canvas, { deltaY: -100, clientX: 200, clientY: 200 })
    }
    expect(Number.parseInt(zoomLabel.textContent ?? '0', 10)).toBeLessThanOrEqual(
      Math.round(MAX_CAMERA_ZOOM * 100)
    )

    for (let i = 0; i < 30; i += 1) {
      fireEvent.wheel(canvas, { deltaY: 100, clientX: 200, clientY: 200 })
    }
    expect(Number.parseInt(zoomLabel.textContent ?? '0', 10)).toBeGreaterThanOrEqual(
      Math.round(MIN_CAMERA_ZOOM * 100)
    )

    // Панорамирование по СКМ не вызывает ошибок и переключает класс.
    fireEvent.pointerDown(canvas, { button: 1, clientX: 100, clientY: 100, pointerId: 1 })
    expect(screen.getByTestId('hive-city-scene').className).toContain('hive-city-scene--panning')
    fireEvent.pointerMove(canvas, { clientX: 160, clientY: 140, pointerId: 1 })
    fireEvent.pointerUp(canvas, { button: 1, pointerId: 1 })
    expect(screen.getByTestId('hive-city-scene').className).not.toContain('hive-city-scene--panning')
  })
})
