/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { test, expect } from 'vitest'
import { TeamResourceBudgetPanel } from '../src/renderer/src/TeamResourceBudgetPanel'

test('team resource budget is metadata-only', () => { render(<TeamResourceBudgetPanel connection="disconnected" events={[]} />); expect(screen.getByText('Owner scope')).toBeTruthy(); expect(screen.getByText(/Core владеет envelope/)).toBeTruthy(); expect(screen.queryByText(/host path/i)).toBeNull() })
