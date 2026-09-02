/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { TypedAgentHandoffContractPanel } from '../src/renderer/src/TypedAgentHandoffContractPanel'
test('handoff panel displays lifecycle without authority fields', () => { render(<TypedAgentHandoffContractPanel connection="disconnected" events={[]} />); expect(screen.getByText(/Lifecycle ownership transfer/)).toBeTruthy(); expect(screen.queryByText(/capabilities не наследуются/)).toBeTruthy() })
