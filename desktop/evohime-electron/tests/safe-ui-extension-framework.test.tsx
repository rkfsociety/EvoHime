// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { SafeUiExtensionFrameworkPanel } from '../src/renderer/src/SafeUiExtensionFrameworkPanel'

describe('SafeUiExtensionFrameworkPanel', () => {
  it('renders the metadata-only extension controls while disconnected', () => {
    render(<SafeUiExtensionFrameworkPanel connection="disconnected" />)

    expect(screen.getByRole('region', { name: 'Safe UI Extension Framework' })).toBeTruthy()
    expect(screen.getByRole('textbox', { name: 'UI extension manifest JSON' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'install' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'validate' })).toBeTruthy()
  })
})
