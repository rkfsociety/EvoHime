// @vitest-environment jsdom
import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { IntegrationProviderPanel } from '../src/renderer/src/IntegrationProviderPanel'

describe('Integration Provider SDK projection', () => {
  it('requests catalog metadata and never renders secret material', async () => {
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    Object.defineProperty(window, 'evohime', { configurable: true, value: { v1: { invoke } } })
    render(<IntegrationProviderPanel />)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('integrationProvider.listCatalog', expect.any(Object)))
    expect(screen.getByText('fixture.echo · offline')).toBeTruthy()
    expect(document.body.textContent).not.toContain('secret')
  })
})
