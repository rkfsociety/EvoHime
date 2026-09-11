// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { expect, it } from 'vitest'

import type { CoreEvent } from '../src/shared/api'
import { OllamaModelDownloadPanel } from '../src/renderer/src/OllamaModelDownloadPanel'

function event(eventType: string, payload: string): CoreEvent {
  return { sequenceId: 1, taskId: '', eventType, payload } as CoreEvent
}

it('показывает размер модели и прогресса в правильных единицах', () => {
  const catalog = event('model.catalog', JSON.stringify({
    ollama: {
      recommendations: [{
        id: 'qwen3:0.6b',
        description: 'быстрая базовая модель',
        size_bytes: 498 * 1024 * 1024,
        required_ram_bytes: 2 * 1024 * 1024 * 1024,
        fits_device: true,
        installed: false,
        reason: 'подходит'
      }],
      installed: ['qwen2.5:3b']
    }
  }))
  const progress = {
    ...event('local-model-runtime-manager', ''),
    localModelRuntimeManager: {
      schemaVersion: 1,
      operation: 'ollama_pull',
      version: 1,
      status: 'running',
      errorCode: '',
      projection: {
        status: 'downloading',
        model: 'qwen3:0.6b',
        stage: 'pulling layer',
        completed_bytes: 124 * 1024 * 1024,
        total_bytes: 498 * 1024 * 1024,
        percent: 24,
        redacted: true
      }
    }
  } as CoreEvent

  render(<OllamaModelDownloadPanel connection="disconnected" events={[progress, catalog]} baseUrl="" />)

  expect(screen.getByText(/24% · 124 МБ из 498 МБ/)).toBeTruthy()
  expect(screen.getByText(/498 МБ · подходит/)).toBeTruthy()
  expect(screen.getByText('qwen2.5:3b')).toBeTruthy()
  expect(screen.getByText('установлена в Ollama · доступна в композиторе')).toBeTruthy()
})
