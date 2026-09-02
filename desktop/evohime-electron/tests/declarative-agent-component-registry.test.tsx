// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { DeclarativeAgentComponentRegistryPanel } from '../src/renderer/src/DeclarativeAgentComponentRegistryPanel'

describe('DeclarativeAgentComponentRegistryPanel', () => {
  it('shows bounded inspect, diff and migration actions', () => {
    render(<DeclarativeAgentComponentRegistryPanel connection="disconnected" />)
    expect(screen.getByRole('region', { name: 'Declarative Agent Component Registry' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'diff' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'validate' })).toBeTruthy()
  })
})
