// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import type { CapabilityRecipeCatalog, CapabilityRecipePreflight, CoreEvent, EvoHimeApiV1, RendererCommand } from '../src/shared/api'
import { CapabilityRecipePanel } from '../src/renderer/src/CapabilityRecipePanel'

const calls: Array<{ command: string; payload: unknown }> = []

beforeEach(() => {
  calls.length = 0
  const api: EvoHimeApiV1 = {
    apiVersion: 1,
    invoke: (async (command: RendererCommand, payload: unknown) => {
      calls.push({ command, payload })
      return { ok: true, value: { accepted: true } } as never
    }) as EvoHimeApiV1['invoke'],
    subscribe: () => () => {},
    writeClipboardText: async () => true,
    openExternal: async () => true,
    pathForFile: () => '',
  }
  Object.defineProperty(window, 'evohime', { value: Object.freeze({ v1: api }), configurable: true })
})

afterEach(() => cleanup())

function catalogEvent(payload: CapabilityRecipeCatalog): CoreEvent {
  return {
    sequenceId: 1,
    taskId: '',
    eventType: 'capability_recipe.catalog',
    payload: JSON.stringify(payload),
  }
}

describe('CapabilityRecipePanel', () => {
  it('keeps typed unsupported recipes unstartable even if a preflight response says ready', async () => {
    const catalog: CapabilityRecipeCatalog = {
      catalog_version: 1,
      error_code: '',
      recipes: [{
        id: 'model-comparison',
        version: 1,
        category: 'model_comparison',
        difficulty: 'intermediate',
        title: 'Сравнение моделей',
        description: 'Для этого сценария нет совместимого исполнителя.',
        inputs: [{ name: 'goal', title: 'Цель', required: true, max_chars: 512 }],
        required_capabilities: [],
        optional_capabilities: [],
        workflow_binding: null,
        preview: ['Нет совместимого workflow adapter в этой версии.'],
        availability: { status: 'unsupported', reason_code: 'model_run_adapter_unavailable' },
        content_hash: 'sha256:recipe',
      }],
    }

    const props = {
      connection: 'connected' as const,
      workspace: 'C:\\work',
      onOpenDraft: () => {},
    }
    const { rerender } = render(
      <CapabilityRecipePanel
        events={[catalogEvent(catalog)]}
        {...props}
      />,
    )

    expect(await screen.findByText('Запуск недоступен: model_run_adapter_unavailable')).toBeTruthy()
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'compare' } })
    fireEvent.click(screen.getByRole('button', { name: 'Проверить' }))
    await waitFor(() => expect(calls.some((call) => call.command === 'capabilityRecipe.preflight')).toBe(true))

    const readyPreflight: CapabilityRecipePreflight = {
      catalog_version: 1,
      recipe_id: 'model-comparison',
      recipe_version: 1,
      recipe_hash: 'sha256:recipe',
      state: 'ready',
      reason_codes: [],
      workflow_binding: null,
      input_hash: 'input-hash',
      workspace_hash: 'workspace-hash',
      run_graph_hash: 'graph-hash',
      preview: [],
      required_capabilities: [],
      optional_capabilities: [],
      revisions: [],
      workflow_budget: null,
      approval_points: [],
      degraded_paths: [],
      preflight_hash: 'preflight-hash',
      error_code: '',
    }
    rerender(
      <CapabilityRecipePanel
        events={[
          catalogEvent(catalog),
          {
            sequenceId: 2,
            taskId: '',
            eventType: 'capability_recipe.preflight',
            payload: JSON.stringify(readyPreflight),
          },
        ]}
        {...props}
      />,
    )

    expect(await screen.findByText('Проверка Core: готово к запуску')).toBeTruthy()
    const startButton = screen.getByRole('button', { name: 'Запустить' }) as HTMLButtonElement
    expect(startButton.disabled).toBe(true)
    expect(calls.some((call) => call.command === 'capabilityRecipe.list')).toBe(true)
    expect(calls.some((call) => call.command === 'capabilityRecipe.start')).toBe(false)
  })
})
