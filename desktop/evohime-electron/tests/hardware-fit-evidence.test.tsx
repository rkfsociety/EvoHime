// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { HardwareFitEvidencePanel } from '../src/renderer/src/HardwareFitEvidencePanel'

describe('HardwareFitEvidencePanel', () => {
  it('requests bounded metadata and never renders sensitive payloads', async () => {
    const invoke = vi.fn().mockResolvedValue({ accepted: true })
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe: vi.fn().mockReturnValue(() => undefined) } } })
    render(<HardwareFitEvidencePanel />)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(invoke).toHaveBeenCalledWith('hardwareFitEvidence.list', expect.any(Object))
    expect(screen.getByText(/Core contract v1/)).toBeTruthy()
    expect(document.body.textContent).not.toMatch(/sk-[a-z0-9]{20,}|Bearer\s+[A-Za-z0-9._-]+/i)
  })
})
