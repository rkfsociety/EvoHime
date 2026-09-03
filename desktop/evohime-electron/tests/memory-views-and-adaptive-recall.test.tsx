/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { MemoryViewsAndAdaptiveRecallPanel } from '../src/renderer/src/MemoryViewsAndAdaptiveRecallPanel'

test('memory views panel exposes only bounded Core projection actions', () => {
  render(<MemoryViewsAndAdaptiveRecallPanel connection="disconnected" events={[]} />)
  expect(screen.getByRole('region', { name: 'Memory Views and Adaptive Recall' })).toBeTruthy()
  expect(screen.getByRole('button', { name: 'save_view' })).toBeTruthy()
  expect(screen.getByRole('button', { name: 'recall' })).toBeTruthy()
  expect(screen.getByText(/Core ограничивает scope/)).toBeTruthy()
  expect(screen.queryByText(/prompt|credentials|raw output/i)).toBeNull()
})
