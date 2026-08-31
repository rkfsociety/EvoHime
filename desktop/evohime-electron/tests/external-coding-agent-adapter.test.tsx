// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ExternalCodingAgentAdapterPanel } from '../src/renderer/src/ExternalCodingAgentAdapterPanel'

describe('ExternalCodingAgentAdapterPanel', () => {
  it('requests metadata-only status and exposes opaque control honestly', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    const subscribe = vi.fn().mockReturnValue(() => undefined)
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe } } })
    render(<ExternalCodingAgentAdapterPanel />)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(invoke).toHaveBeenCalledWith('externalCodingAgentAdapter.status', expect.any(Object))
    expect(screen.getByText(/declared slots/)).toBeTruthy()
    expect(document.body.textContent).not.toMatch(/password|Bearer |raw output|absolute path/i)
  })
})
