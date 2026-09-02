/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { TeamCoordinationPoliciesPanel } from '../src/renderer/src/TeamCoordinationPoliciesPanel'
test('team coordination panel remains a Core projection', () => { render(<TeamCoordinationPoliciesPanel connection="disconnected" events={[]} />); expect(screen.getByText(/Core выбирает роль/)).toBeTruthy(); expect(screen.queryByText(/credentials/i)).toBeNull() })
