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
    fireEvent.change(screen.getByLabelText('Идентификатор плана'), { target: { value: 'plan-1' } })
    fireEvent.click(screen.getByRole('button', { name: 'Прочитать план' }))
    expect(invoke).toHaveBeenCalledWith('core.planArtifactRead', { artifactId: 'plan-1' })
    expect(screen.getByText('Проверить код · риск: low')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Принять план' }))
    expect(await vi.waitFor(() => invoke.mock.calls.some(([command]) => command === 'core.planArtifactAction'))).toBe(true)
  })

  it('explains the purpose and current workflow before a plan is loaded', () => {
    installApi()
    render(<PlanArtifactPanel connection="connected" events={[]} />)

    expect(screen.getByText(/Здесь хранится утверждённый план работы агента/)).toBeTruthy()
    expect(screen.getByText('Для чего это нужно')).toBeTruthy()
    expect(screen.getByText('Как пользоваться')).toBeTruthy()
    expect(screen.getByText(/Сейчас планы создаются другими сценариями Core/)).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Прочитать план' })).toBeTruthy()
  })
})
