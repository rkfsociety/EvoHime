// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import { ProjectSidebar } from '../src/renderer/src/ProjectSidebar'
import { RoutingStatus } from '../src/renderer/src/RoutingStatus'

/**
 * Холодный запуск не должен выглядеть как авария: до первого успешного
 * подключения в сессии связь ещё не терялась, поэтому алармы («Связь с Core
 * потеряна», жёлтая плашка «Core недоступен») не показываются — их место
 * занимает спокойный статус «Запуск» / «Подключаемся к Core…». После первого
 * подключения те же поверхности работают как раньше.
 */

const sidebarProps = {
  workspace: null,
  chatId: null,
  onWorkspaceChange: () => {},
  onChatChange: () => {},
  revision: 0
} as const

const LOST_CONNECTION = 'Связь с Core потеряна: состояние маршрутизации недоступно.'

afterEach(() => cleanup())

describe('cold-start calm connection surface', () => {
  it('hides the lost-connection alarm before the first successful connection', () => {
    render(<RoutingStatus events={[]} connection="connecting" everConnected={false} />)
    expect(screen.queryByText(LOST_CONNECTION)).toBeNull()
  })

  it('still raises the lost-connection alarm after the session reached Core', () => {
    render(<RoutingStatus events={[]} connection="reconnecting" />)
    expect(screen.getByText(LOST_CONNECTION)).toBeTruthy()
  })

  it('keeps the lost-connection alarm visible on a state gap after connection', () => {
    render(<RoutingStatus events={[]} connection="state-gap" />)
    expect(screen.getByText(LOST_CONNECTION)).toBeTruthy()
  })

  it('shows a calm connecting plate in the sidebar on a cold start', () => {
    render(<ProjectSidebar connection="starting" everConnected={false} {...sidebarProps} />)
    expect(screen.getByText('Подключаемся к Core…')).toBeTruthy()
    expect(screen.queryByText('Core недоступен, задачи не запускаются.')).toBeNull()
  })

  it('warns about unavailable Core in the sidebar only after it was connected', () => {
    render(<ProjectSidebar connection="reconnecting" {...sidebarProps} />)
    expect(screen.getByText('Core недоступен, задачи не запускаются.')).toBeTruthy()
    expect(screen.queryByText('Подключаемся к Core…')).toBeNull()
  })
})
