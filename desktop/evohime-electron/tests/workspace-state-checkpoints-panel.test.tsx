/** @vitest-environment jsdom */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { WorkspaceStateCheckpointsPanel } from '../src/renderer/src/WorkspaceStateCheckpointsPanel'

afterEach(() => { cleanup(); delete (window as unknown as { evohime?: unknown }).evohime })

describe('WorkspaceStateCheckpointsPanel', () => {
  it('explains the purpose and risks before an operation is selected', () => {
    ;(window as unknown as { evohime: unknown }).evohime = { v1: { apiVersion: 1, invoke: vi.fn(), subscribe: () => () => {}, writeClipboardText: async () => true } }
    render(<WorkspaceStateCheckpointsPanel connection="connected" events={[]} />)

    expect(screen.getByText(/Это сохранённый снимок состояния файлов проекта/)).toBeTruthy()
    expect(screen.getByText('Как это работает')).toBeTruthy()
    expect(screen.getByText(/Восстановление может изменить файлы/)).toBeTruthy()
    expect(screen.getByLabelText(/^Идентификатор контрольной точки/)).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Создать контрольную точку' })).toBeTruthy()
  })

  it('shows bounded metadata and sends the selected explicit operation', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    ;(window as unknown as { evohime: unknown }).evohime = { v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true } }
    render(<WorkspaceStateCheckpointsPanel connection="connected" events={[{ eventType: 'workspace_state_checkpoint.result', payload: JSON.stringify({ schema_version: 1, operation: 'compare', state: 'compared', fileCount: 2, conflictCount: 0, snapshotHash: 'hash' }) }]} />)
    fireEvent.change(screen.getByLabelText('Идентификатор проекта'), { target: { value: 'project-1' } })
    fireEvent.click(screen.getByRole('button', { name: 'Сравнить с точкой' }))
    expect(await vi.waitFor(() => invoke.mock.calls.some(([command, payload]) => command === 'core.workspaceStateCheckpoint' && payload.operation === 'compare' && payload.projectId === 'project-1'))).toBe(true)
    expect(screen.getByText(/файлов: 2/)).toBeTruthy()
  })
})
