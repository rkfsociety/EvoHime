// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { CapabilityWorkbenchPanel } from '../src/renderer/src/CapabilityWorkbenchPanel'

describe('CapabilityWorkbenchPanel', () => {
  it('renders bounded lifecycle operations while disconnected', () => {
    render(<CapabilityWorkbenchPanel connection="disconnected" />)
    expect(screen.getByRole('region', { name: 'Capability Workbench' })).toBeTruthy()
    expect(screen.getByRole('combobox')).toBeTruthy()
    expect(screen.getByRole('textbox', { name: 'Capability Workbench JSON' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Отправить в Core' })).toBeTruthy()
  })
})
