// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { ArchitectureSnapshotPanel } from '../src/renderer/src/ArchitectureSnapshotPanel'

test('architecture snapshot keeps topology and evidence behind Core', () => {
  render(<ArchitectureSnapshotPanel connection="disconnected" workspace="C:/workspace" />)
  expect(screen.getByText(/только bounded projection/)).toBeTruthy()
  expect(screen.getByRole('option', { name: 'route' })).toBeTruthy()
  expect(screen.getByRole('option', { name: 'review' })).toBeTruthy()
  expect(screen.getByRole('button', { name: 'Отправить в Core' })).toBeTruthy()
})
