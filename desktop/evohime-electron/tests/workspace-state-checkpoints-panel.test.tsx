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
    expect(screen.getByLabelText('Контрольная точка')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Создать контрольную точку' })).toBeTruthy()
  })

  it('shows bounded metadata and sends the selected explicit operation', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    ;(window as unknown as { evohime: unknown }).evohime = { v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true } }
    render(<WorkspaceStateCheckpointsPanel connection="connected" events={[{ eventType: 'workspace_state_checkpoint.result', payload: JSON.stringify({ schema_version: 1, operation: 'compare', state: 'compared', fileCount: 2, conflictCount: 0, snapshotHash: 'hash' }) }]} />)
    fireEvent.change(screen.getByLabelText('Рабочая папка проекта'), { target: { value: 'project-1' } })
    fireEvent.click(screen.getByRole('button', { name: 'Сравнить с точкой' }))
    expect(await vi.waitFor(() => invoke.mock.calls.some(([command, payload]) => command === 'core.workspaceStateCheckpoint' && payload.operation === 'compare' && payload.projectId === 'project-1'))).toBe(true)
    expect(screen.getByText(/файлов: 2/)).toBeTruthy()
  })

  it('renders checkpoint and task choices returned by Core', () => {
    ;(window as unknown as { evohime: unknown }).evohime = { v1: { apiVersion: 1, invoke: vi.fn(), subscribe: () => () => {}, writeClipboardText: async () => true } }
    render(<WorkspaceStateCheckpointsPanel connection="connected" events={[{ eventType: 'workspace_state_checkpoint.result', payload: JSON.stringify({ operation: 'list', checkpoints: [{ checkpoint_id: 'checkpoint-1234', task_id: 'task-1', snapshot_hash: 'hash-1234', created_at_ms: 1, pinned: false }] }) }]} workspace="C:\\work" />)

    expect(screen.getByRole('option', { name: /checkpoi/ })).toBeTruthy()
    expect(screen.getByRole('option', { name: 'task-1' })).toBeTruthy()
  })

  it('shows a readable Core error when the workspace exceeds checkpoint limits', () => {
    ;(window as unknown as { evohime: unknown }).evohime = { v1: { apiVersion: 1, invoke: vi.fn(), subscribe: () => () => {}, writeClipboardText: async () => true } }
    render(<WorkspaceStateCheckpointsPanel connection="connected" events={[{ eventType: 'workspace_state_checkpoint.result', payload: JSON.stringify({ operation: 'create', state: 'failed', error_code: 'workspace_checkpoint_limit_exceeded' }) }]} workspace="C:\\work" />)

    expect(screen.getByRole('alert').textContent).toMatch(/до 4096 файлов, 64 МБ всего/)
  })
})
