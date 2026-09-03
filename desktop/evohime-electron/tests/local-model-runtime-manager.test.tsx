// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { LocalModelRuntimeManagerPanel } from '../src/renderer/src/LocalModelRuntimeManagerPanel'

test('local model manager keeps process launch behind Core and supervisor', () => {
  render(<LocalModelRuntimeManagerPanel connection="disconnected" />)
  expect(screen.getByText(/supervisor boundary/)).toBeTruthy()
  expect(screen.getByRole('button', { name: 'Снять hardware snapshot' })).toBeTruthy()
  expect(screen.getByRole('button', { name: 'Скачать artifact' })).toBeTruthy()
  expect(screen.getByRole('button', { name: 'Рассчитать fit' })).toBeTruthy()
})
