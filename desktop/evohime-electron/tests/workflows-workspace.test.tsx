// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { WorkflowsWorkspace } from '../src/renderer/src/WorkflowsWorkspace'
import type { EvoHimeApiV1 } from '../src/shared/api'

beforeEach(() => {
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: vi.fn(async () => ({ ok: true, value: { accepted: true } })) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true,
    pathForFile: () => '',
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => {
  cleanup()
})

describe('WorkflowsWorkspace', () => {
  it('показывает вкладку «Запуск и сценарии» по умолчанию и скрывает «Создание и отладка»', async () => {
    render(
      <WorkflowsWorkspace
        connection="connected"
        events={[]}
        workspace="C:\\work"
        draftId="builder-draft"
        ownerScope={null}
        onOpenDraft={() => {}}
      />,
    )

    const runTab = screen.getByRole('tab', { name: 'Запуск и сценарии' })
    const studioTab = screen.getByRole('tab', { name: 'Создание и отладка' })

    expect(runTab.getAttribute('aria-selected')).toBe('true')
    expect(studioTab.getAttribute('aria-selected')).toBe('false')

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Шаблоны' })).toBeTruthy())
    expect(document.getElementById('workflows-pane-run')?.hasAttribute('hidden')).toBe(false)
    expect(document.getElementById('workflows-pane-studio')?.hasAttribute('hidden')).toBe(true)
  })

  it('переключается на «Создание и отладка» и сохраняет состояние при переключении обратно', async () => {
    render(
      <WorkflowsWorkspace
        connection="connected"
        events={[]}
        workspace="C:\\work"
        draftId="builder-draft"
        ownerScope={null}
        onOpenDraft={() => {}}
      />,
    )

    const studioTab = screen.getByRole('tab', { name: 'Создание и отладка' })
    fireEvent.click(studioTab)

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Composer workflow' })).toBeTruthy())
    expect(studioTab.getAttribute('aria-selected')).toBe('true')
    expect(document.getElementById('workflows-pane-studio')?.hasAttribute('hidden')).toBe(false)
    expect(document.getElementById('workflows-pane-run')?.hasAttribute('hidden')).toBe(true)

    const composerInput = screen.getByLabelText('Описание workflow') as HTMLTextAreaElement
    fireEvent.change(composerInput, { target: { value: 'Мой черновик' } })

    fireEvent.click(screen.getByRole('tab', { name: 'Запуск и сценарии' }))
    expect(document.getElementById('workflows-pane-run')?.hasAttribute('hidden')).toBe(false)
    expect(document.getElementById('workflows-pane-studio')?.hasAttribute('hidden')).toBe(true)

    fireEvent.click(studioTab)
    expect((screen.getByLabelText('Описание workflow') as HTMLTextAreaElement).value).toBe('Мой черновик')
  })
})
