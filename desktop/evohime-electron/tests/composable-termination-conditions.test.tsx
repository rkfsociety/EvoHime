/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { ComposableTerminationConditionsPanel } from '../src/renderer/src/ComposableTerminationConditionsPanel'
test('termination panel is a Core projection', () => { render(<ComposableTerminationConditionsPanel connection="disconnected" events={[]} />); expect(screen.getByText('Owner scope')).toBeTruthy(); expect(screen.getByText(/continuation не переопределяет hard stop/)).toBeTruthy() })
