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
    expect(screen.getByRole('button', { name: 'Проверить llama.cpp adapter' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Установить фиксированный llama.cpp adapter' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Создать задачу адаптации из JSON' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Запустить quantization задачи из JSON' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Обновить состояние quantization из JSON' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Проверить streamed inference из JSON' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Получить задачу адаптации из JSON' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Список задач адаптации' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Отменить задачу адаптации из JSON' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Отклонить задачу адаптации из JSON' })).toBeTruthy()
})
