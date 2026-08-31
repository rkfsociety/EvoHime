// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { TeamSopProtocolsPanel } from '../src/renderer/src/TeamSopProtocolsPanel'

describe('TeamSopProtocolsPanel', () => {
  it('requests and displays only bounded metadata', async () => {
    const invoke = vi.fn().mockResolvedValue({ accepted: true })
    const subscribe = vi.fn().mockReturnValue(() => undefined)
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe } } })
    render(<TeamSopProtocolsPanel />)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(invoke).toHaveBeenCalledWith('teamSopProtocols.list', expect.any(Object))
    expect(screen.getByText(/storage v49/)).toBeTruthy()
    expect(document.body.textContent).not.toMatch(/sk-[a-z0-9]{20,}|Bearer\s+[A-Za-z0-9._-]+/i)
  })
})
