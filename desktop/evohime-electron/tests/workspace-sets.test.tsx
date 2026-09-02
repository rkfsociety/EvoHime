// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { WorkspaceSetsPanel } from '../src/renderer/src/WorkspaceSetsPanel'

vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))

describe('Workspace Sets panel', () => {
  it('exposes a projection-only multi-root surface', () => {
    render(<WorkspaceSetsPanel connection="starting" />)
    expect(screen.getByRole('region', { name: 'Workspace Sets' })).toBeTruthy()
    expect(screen.getByLabelText('Workspace Set JSON')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Отправить в Core' })).toBeTruthy()
  })
})
