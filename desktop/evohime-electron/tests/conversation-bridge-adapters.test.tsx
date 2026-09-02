// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'

vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))

import { ConversationBridgeAdaptersPanel } from '../src/renderer/src/ConversationBridgeAdaptersPanel'

describe('ConversationBridgeAdaptersPanel', () => {
  it('exposes only Core-owned metadata actions', () => {
    render(<ConversationBridgeAdaptersPanel connection="disconnected" />)
    expect(screen.getByRole('region', { name: 'Conversation Bridge Adapters' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Показать состояние' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Отозвать bridge' })).toBeTruthy()
    expect(screen.getByText(/credentials/)).toBeTruthy()
  })
})
