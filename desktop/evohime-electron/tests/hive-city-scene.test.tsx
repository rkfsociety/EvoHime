// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { HiveCityScene, type HiveCityDepartmentPin } from '../src/renderer/src/components/3d'

describe('HiveCityScene', () => {
  afterEach(() => {
    cleanup()
  })

  it('renders 2.5D city backdrop, fx layer, normalized status, and aria-hidden in decorative mode', () => {
    const { rerender } = render(<HiveCityScene status="observing" />)

    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('data-status')).toBe('observing')
    expect(scene.getAttribute('aria-hidden')).toBe('true')
    expect(screen.getByTestId('hive-city-scene-art')).toBeTruthy()
    expect(screen.getByTestId('hive-city-scene-fx')).toBeTruthy()

    rerender(<HiveCityScene status="active" />)
    expect(screen.getByTestId('hive-city-scene').getAttribute('data-status')).toBe('active')

    rerender(<HiveCityScene status="" />)
    expect(screen.getByTestId('hive-city-scene').getAttribute('data-status')).toBe('idle')
  })

  it('renders interactive department pins, highlights active conduits, and emits selection callbacks', async () => {
    const user = userEvent.setup()
    const onSelectDepartment = vi.fn()
    const departments: readonly HiveCityDepartmentPin[] = [
      {
        key: 'development',
        label: 'Разработка',
        subtitle: 'Разработка кода',
        icon: 'terminal',
        activeCount: 2,
        maxCapacity: 4
      },
      {
        key: 'research',
        label: 'Исследования',
        subtitle: 'Поиск решений',
        icon: 'target',
        activeCount: 0,
        maxCapacity: 4
      }
    ]

    render(
      <HiveCityScene
        status="active"
        departments={departments}
        selectedDepartment="development"
        onSelectDepartment={onSelectDepartment}
      />
    )

    const scene = screen.getByTestId('hive-city-scene')
    expect(scene.getAttribute('aria-hidden')).toBeNull()

    const devPin = screen.getByTestId('hive-pin-development')
    const researchPin = screen.getByTestId('hive-pin-research')
    expect(devPin.getAttribute('aria-pressed')).toBe('true')
    expect(devPin.textContent).toContain('Работает · 2/4')
    expect(researchPin.getAttribute('aria-pressed')).toBe('false')
    expect(researchPin.textContent).toContain('Ожидание')

    const conduits = screen.getAllByTestId('hive-city-conduit')
    const devConduit = conduits.find((item) => item.getAttribute('data-dept') === 'development')
    const researchConduit = conduits.find((item) => item.getAttribute('data-dept') === 'research')
    expect(devConduit?.getAttribute('data-active')).toBe('true')
    expect(researchConduit?.getAttribute('data-active')).toBe('false')

    await user.click(researchPin)
    expect(onSelectDepartment).toHaveBeenCalledWith('research')
  })
})
