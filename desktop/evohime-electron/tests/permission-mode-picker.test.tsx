// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CommandOutcome, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { PermissionModePicker } from '../src/renderer/src/PermissionModePicker'

const calls: Array<{ command: string; payload: unknown }> = []

function ok<C extends RendererCommand>(value: unknown): CommandOutcome<C> {
  return { ok: true, value } as CommandOutcome<C>
}

beforeEach(() => {
  calls.length = 0
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      return ok({ accepted: true })
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

describe('permission mode picker', () => {
  it('shows the current mode and applies a selected mode through Core', async () => {
    render(<PermissionModePicker connection="connected" />)

    await userEvent.click(screen.getByRole('button', { name: 'Режим доступа' }))
    expect(screen.getAllByText('Запрашивать разрешение').length).toBe(2)
    await userEvent.click(screen.getByRole('menuitemradio', { name: /Полный доступ/ }))

    expect(calls).toContainEqual({ command: 'core.setPermissionMode', payload: { mode: 'full' } })
    expect(screen.getByRole('button', { name: 'Режим доступа' }).textContent).toContain('Полный доступ')
  })
})
