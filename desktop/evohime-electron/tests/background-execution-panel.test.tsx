// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { BackgroundExecutionPanel } from '../src/renderer/src/BackgroundExecutionPanel'

test('background execution panel keeps detached state Core-owned and redacted', () => {
  render(<BackgroundExecutionPanel connection="disconnected" events={[]} />)
  expect(screen.getByRole('region', { name: 'Durable Background Execution' })).toBeTruthy()
  expect(screen.getByText(/Core-owned detached runs/)).toBeTruthy()
  expect(screen.getByText(/При отключённом Core verdict не вычисляется/)).toBeTruthy()
})
