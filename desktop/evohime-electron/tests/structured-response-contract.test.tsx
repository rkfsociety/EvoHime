/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { StructuredResponseContractPanel } from '../src/renderer/src/StructuredResponseContractPanel'

afterEach(cleanup)

describe('StructuredResponseContractPanel', () => {
  it('shows Core-owned bounded projection', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    vi.stubGlobal('window', { evohime: { v1: { invoke, subscribe: vi.fn(() => () => {}) } } })
    render(<StructuredResponseContractPanel />)
    expect(await screen.findByText(/Контракт доступен в Core/)).toBeTruthy()
    expect(screen.getByText(/raw output не попадает/)).toBeTruthy()
  })
})
