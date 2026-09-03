// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { DurableRemoteTaskBridgePanel } from '../src/renderer/src/DurableRemoteTaskBridgePanel'

vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))

describe('Durable Remote Task Bridge panel', () => {
  it('exposes bounded Core-owned lifecycle controls', () => {
    render(<DurableRemoteTaskBridgePanel connection="disconnected" />)
    expect(screen.getByRole('region', { name: 'Durable Remote Task Bridge' })).toBeTruthy()
    expect(screen.getByRole('option', { name: 'cancel' })).toBeTruthy()
    expect(screen.getByLabelText('Durable Remote Task Bridge JSON')).toBeTruthy()
  })
})
