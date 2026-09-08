// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { AgentGitChangeSetsPanel } from '../src/renderer/src/AgentGitChangeSetsPanel'

vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))

describe('Agent Git Change Sets panel', () => {
  it('keeps workspace identity and actions as a Core projection surface', () => {
    render(<AgentGitChangeSetsPanel connection="starting" />)
    expect(screen.getByRole('region', { name: 'Agent Git Change Sets' })).toBeTruthy()
    expect(screen.getByLabelText('Workspace root')).toBeTruthy()
    expect(screen.getByLabelText('Agent Git Change Sets JSON')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Отправить в Core' })).toBeTruthy()
  })
})
