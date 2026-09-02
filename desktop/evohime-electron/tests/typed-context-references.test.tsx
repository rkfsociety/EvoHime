// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { TypedContextReferencesPanel } from '../src/renderer/src/TypedContextReferencesPanel'

describe('TypedContextReferencesPanel', () => {
  it('shows typed resolver and bounded budget actions', () => {
    render(<TypedContextReferencesPanel connection="disconnected" />)
    expect(screen.getByRole('region', { name: 'Typed Context References' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'resolve' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'budget' })).toBeTruthy()
  })
})
