// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { ContextNamespacePanel } from '../src/renderer/src/ContextNamespacePanel'

test('context namespace panel is read-only and exposes bounded trace metadata', () => {
  render(<ContextNamespacePanel connection="disconnected" events={[]} />)

  expect(screen.getByRole('region', { name: 'Context Namespace Explorer' })).toBeTruthy()
  expect(screen.getByText(/Logical path не является ACL/)).toBeTruthy()
  expect(screen.getByRole('button', { name: 'list_children' }).hasAttribute('disabled')).toBe(true)
  expect(screen.getByText(/Trace:.*нет данных/)).toBeTruthy()
})
