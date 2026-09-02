// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { ProjectInstructionStackPanel } from '../src/renderer/src/ProjectInstructionStackPanel'

describe('ProjectInstructionStackPanel', () => {
  it('renders Core-owned discovery and bounded projection controls', () => {
    render(<ProjectInstructionStackPanel connection="disconnected" />)
    expect(screen.getByRole('region', { name: 'Project Instruction Stack' })).toBeTruthy()
    expect(screen.getByRole('textbox', { name: 'Relevant project paths' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Отправить в Core' })).toBeTruthy()
  })
})
