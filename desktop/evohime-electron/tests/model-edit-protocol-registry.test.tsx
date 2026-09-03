/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ModelEditProtocolRegistryPanel } from '../src/renderer/src/ModelEditProtocolRegistryPanel'

vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))

describe('model edit protocol registry panel', () => {
  it('exposes bounded Core actions without raw output authority', () => {
    render(<ModelEditProtocolRegistryPanel connection="disconnected" events={[]} />)
    expect(screen.getByRole('region', { name: 'Model Edit Protocol Registry' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'preflight' })).toBeTruthy()
    expect(screen.queryByText(/credentials|hidden reasoning|shell command/i)).toBeNull()
  })
})
