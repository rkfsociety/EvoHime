// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ExecutionPolicyProfilesPanel } from '../src/renderer/src/ExecutionPolicyProfilesPanel'

describe('ExecutionPolicyProfilesPanel', () => {
  it('requests and renders only the bounded Core projection', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    const subscribe = vi.fn().mockReturnValue(() => undefined)
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke, subscribe } } })
    render(<ExecutionPolicyProfilesPanel />)
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(invoke).toHaveBeenCalledWith('executionPolicyProfiles.status', expect.any(Object))
    expect(screen.getByText(/Ожидание состояния Core/)).toBeTruthy()
    expect(document.body.textContent).not.toMatch(/EVOHIME_API_TOKEN|raw command|password/i)
  })
})
