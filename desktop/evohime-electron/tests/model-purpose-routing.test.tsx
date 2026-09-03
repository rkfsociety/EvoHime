// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { ModelPurposeRoutingPanel } from '../src/renderer/src/ModelPurposeRoutingPanel'

test('model purpose routing panel keeps routing and resilience separate', () => {
  render(<ModelPurposeRoutingPanel connection="disconnected" />)
  expect(screen.getByText(/retry\/fallback остаются/)).toBeTruthy()
  expect(screen.getByRole('button', { name: 'Загрузить policy' })).toBeTruthy()
  expect(screen.getByRole('button', { name: 'Сохранить policy' })).toBeTruthy()
})
