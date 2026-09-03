// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { test, expect } from 'vitest'
import { MessageInterventionPoliciesPanel } from '../src/renderer/src/MessageInterventionPoliciesPanel'

test('message intervention panel is a Core projection', () => {
  render(<MessageInterventionPoliciesPanel connection="disconnected" />)
  expect(screen.getByText(/Перехват выполняется Core до доставки/)).toBeTruthy()
  expect(screen.getByRole('button', { name: 'Проверить доставку' })).toBeTruthy()
})
