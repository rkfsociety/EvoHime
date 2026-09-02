// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TeamCoordinatorPanel } from '../src/renderer/src/TeamCoordinatorPanel'

describe('TeamCoordinatorPanel', () => {
  it('renders bounded Core-owned coordinator controls while disconnected', () => {
    render(<TeamCoordinatorPanel connection="disconnected" />)
    expect(screen.getByRole('region', { name: 'Team Coordinator' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Отправить в Core' })).toBeTruthy()
  })
})
