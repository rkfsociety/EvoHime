// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { PersistentAgentOrganizationRegistryPanel } from '../src/renderer/src/PersistentAgentOrganizationRegistryPanel'

test('persistent agent organization stays as a bounded Core projection', () => {
  render(<PersistentAgentOrganizationRegistryPanel connection="disconnected" />)
  expect(screen.getByRole('region', { name: 'Persistent Agent Organization Registry' })).toBeTruthy()
  expect(screen.getByRole('option', { name: 'reporting_set' })).toBeTruthy()
  expect(screen.getByRole('option', { name: 'assignment_create' })).toBeTruthy()
  expect(screen.getByText(/Runtime, grants, credentials и raw output/)).toBeTruthy()
})
