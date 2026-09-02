// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'

vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))

import { PrivacyTelemetryGovernancePanel } from '../src/renderer/src/PrivacyTelemetryGovernancePanel'

describe('PrivacyTelemetryGovernancePanel', () => {
  it('shows metadata-only queue and consent lifecycle actions', () => {
    render(<PrivacyTelemetryGovernancePanel connection="disconnected" />)
    expect(screen.getByRole('region', { name: 'Privacy Telemetry Governance' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Показать очередь' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Отозвать consent' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Очистить очередь' })).toBeTruthy()
    expect(screen.getByText(/внешняя телеметрия закрыта/)).toBeTruthy()
  })
})
