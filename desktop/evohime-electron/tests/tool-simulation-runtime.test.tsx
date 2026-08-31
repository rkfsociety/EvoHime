// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ToolSimulationRuntimePanel } from '../src/renderer/src/ToolSimulationRuntimePanel'

describe('ToolSimulationRuntimePanel', () => {
  it('requests a metadata-only status and keeps the synthetic boundary visible', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    const subscribe = vi.fn().mockReturnValue(() => undefined)
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe } } })
    render(<ToolSimulationRuntimePanel />)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(invoke).toHaveBeenCalledWith('toolSimulationRuntime.status', expect.any(Object))
    expect(screen.getByText(/это не подтверждение реального эффекта/)).toBeTruthy()
    expect(document.body.textContent).not.toMatch(/password|Bearer |raw output|absolute path/i)
  })
})
