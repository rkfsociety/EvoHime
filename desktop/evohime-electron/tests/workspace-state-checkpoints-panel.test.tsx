/** @vitest-environment jsdom */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { WorkspaceStateCheckpointsPanel } from '../src/renderer/src/WorkspaceStateCheckpointsPanel'

afterEach(() => { cleanup(); delete (window as unknown as { evohime?: unknown }).evohime })

describe('WorkspaceStateCheckpointsPanel', () => {
  it('shows bounded metadata and sends the selected explicit operation', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    ;(window as unknown as { evohime: unknown }).evohime = { v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true } }
    render(<WorkspaceStateCheckpointsPanel connection="connected" events={[{ eventType: 'workspace_state_checkpoint.result', payload: JSON.stringify({ schema_version: 1, operation: 'compare', state: 'compared', fileCount: 2, conflictCount: 0, snapshotHash: 'hash' }) }]} />)
    fireEvent.change(screen.getByLabelText('Проект'), { target: { value: 'project-1' } })
    fireEvent.click(screen.getByRole('button', { name: 'Сравнить' }))
    expect(await vi.waitFor(() => invoke.mock.calls.some(([command, payload]) => command === 'core.workspaceStateCheckpoint' && payload.operation === 'compare' && payload.projectId === 'project-1'))).toBe(true)
    expect(screen.getByText(/файлов: 2/)).toBeTruthy()
  })
})
