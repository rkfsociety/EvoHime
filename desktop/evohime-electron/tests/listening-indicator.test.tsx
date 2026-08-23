// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import type { CoreEvent } from '../src/shared/api'
import { ListeningIndicator } from '../src/renderer/src/App'

function event(eventType: string, payload: Record<string, unknown>): CoreEvent {
  return { sequenceId: 0, taskId: '', eventType, payload: JSON.stringify(payload) }
}

afterEach(() => cleanup())

describe('индикатор слушания в шапке', () => {
  it('берёт состояние из самого нового события, а не из самого старого в буфере', () => {
    // App.tsx кладёт новое событие в начало events ([event, ...current]), так
    // что первое совпадение — самое свежее. Раньше индикатор брал
    // .filter().at(-1) — самое старое ещё не вытесненное совпадение — и на
    // длинной сессии продолжал показывать «проверка состояния», даже когда
    // и ядро, и листенер давно слушали.
    render(
      <ListeningIndicator
        events={[
          event('ambient.state', { state: 'listening', reason: 'user_request' }),
          event('ambient.state', { state: 'engine_unavailable', reason: 'engine_unavailable' })
        ]}
      />
    )
    expect(screen.getByText('Ева слушает')).toBeTruthy()
    expect(screen.queryByText('Слушание: проверка состояния…')).toBeNull()
  })

  it('без единого события честно говорит «проверка состояния», а не «выключено»', () => {
    render(<ListeningIndicator events={[]} />)
    expect(screen.getByText('Слушание: проверка состояния…')).toBeTruthy()
  })
})
