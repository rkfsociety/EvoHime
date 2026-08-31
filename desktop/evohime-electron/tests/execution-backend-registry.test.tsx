// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ExecutionBackendRegistryPanel } from '../src/renderer/src/ExecutionBackendRegistryPanel'

describe('ExecutionBackendRegistryPanel', () => {
  it('requests only the bounded Core projection', async () => {
    const invoke = vi.fn().mockResolvedValue({ accepted: true })
    const subscribe = vi.fn().mockReturnValue(() => undefined)
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe } } })
    render(<ExecutionBackendRegistryPanel />)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(invoke).toHaveBeenCalledWith('executionBackendRegistry.list', expect.any(Object))
    expect(screen.getByText(/Ожидание состояния Core/)).toBeTruthy()
    expect(document.body.textContent).not.toMatch(/token|password|raw prompt|absolute path/i)
  })
})
