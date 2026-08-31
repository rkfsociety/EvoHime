// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { SensitiveDataGuardrailsPanel } from '../src/renderer/src/SensitiveDataGuardrailsPanel'

describe('SensitiveDataGuardrailsPanel', () => {
  it('requests only bounded metadata through Core', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke } } })
    render(<SensitiveDataGuardrailsPanel />)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(invoke).toHaveBeenCalledWith('sensitiveDataGuardrails.status', expect.any(Object))
    expect(screen.getByText(/Сырые prompt\/output/)).toBeTruthy()
    expect(document.body.textContent).not.toMatch(/user@example\.com|sk-[A-Za-z0-9]/)
  })
})
