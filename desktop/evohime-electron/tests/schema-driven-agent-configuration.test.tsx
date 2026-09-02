// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { SchemaDrivenAgentConfigurationPanel } from '../src/renderer/src/SchemaDrivenAgentConfigurationPanel'

describe('SchemaDrivenAgentConfigurationPanel', () => {
  it('exposes Core-owned schema actions and redaction contract', () => {
    render(<SchemaDrivenAgentConfigurationPanel connection="disconnected" events={[]} />)
    expect(screen.getByRole('region', { name: 'Schema-Driven Agent Configuration' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Получить схему' })).toBeTruthy()
    expect(screen.getByText(/secret fields/i)).toBeTruthy()
  })
})
