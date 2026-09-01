/** @vitest-environment jsdom */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { PlanArtifactPanel } from '../src/renderer/src/PlanArtifactPanel'

const projection = { schemaVersion: 1, id: 'plan-1', revision: 1, version: 1, status: 'draft', contentHash: 'hash', steps: [{ id: 'step-1', description: 'Проверить код', risk: 'low' }], acceptanceCriteria: [{ id: 'criterion-1', description: 'Тесты проходят', evidenceKind: 'TestsPass', required: true }] }

function installApi() { const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } }); (window as unknown as { evohime: unknown }).evohime = { v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true } }; return invoke }
afterEach(() => { cleanup(); delete (window as unknown as { evohime?: unknown }).evohime })

describe('PlanArtifactPanel', () => {
  it('renders bounded projection and sends explicit Core action', async () => {
    const invoke = installApi()
    render(<PlanArtifactPanel connection="connected" events={[{ eventType: 'plan_artifact.result', payload: JSON.stringify(projection) }]} />)
    fireEvent.change(screen.getByLabelText('Идентификатор'), { target: { value: 'plan-1' } })
    fireEvent.click(screen.getByRole('button', { name: 'Прочитать' }))
    expect(invoke).toHaveBeenCalledWith('core.planArtifactRead', { artifactId: 'plan-1' })
    expect(screen.getByText('Проверить код (low)')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Принять план' }))
    expect(await vi.waitFor(() => invoke.mock.calls.some(([command]) => command === 'core.planArtifactAction'))).toBe(true)
  })
})
