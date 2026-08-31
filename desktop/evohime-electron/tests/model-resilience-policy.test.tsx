// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ModelResiliencePolicyPanel } from '../src/renderer/src/ModelResiliencePolicyPanel'

describe('ModelResiliencePolicyPanel', () => {
  it('requests and renders only the bounded Core projection', async () => {
    const invoke = vi.fn().mockResolvedValue({ accepted: true })
    const subscribe = vi.fn().mockReturnValue(() => undefined)
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe } } })
    render(<ModelResiliencePolicyPanel />)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(invoke).toHaveBeenCalledWith('modelResiliencePolicy.status', expect.any(Object))
    expect(screen.getByText(/Ожидание состояния Core/)).toBeTruthy()
  })
})
