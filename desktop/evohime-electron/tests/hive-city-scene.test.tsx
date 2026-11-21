// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import { HiveCityScene } from '../src/renderer/src/components/3d'

describe('HiveCityScene', () => {
  afterEach(() => {
    cleanup()
  })

  it('renders scene container with normalized status and safe fallback without WebGL', () => {
    const { rerender } = render(<HiveCityScene status="observing" />)

    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('data-status')).toBe('observing')
    expect(scene.getAttribute('aria-hidden')).toBe('true')
    expect(screen.getByTestId('hive-city-scene-fallback')).toBeTruthy()

    rerender(<HiveCityScene status="active" />)
    expect(screen.getByTestId('hive-city-scene').getAttribute('data-status')).toBe('active')

    rerender(<HiveCityScene status="" />)
    expect(screen.getByTestId('hive-city-scene').getAttribute('data-status')).toBe('idle')
  })
})
