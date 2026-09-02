// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { CodeDiagnosticsFeedbackLoopPanel } from '../src/renderer/src/CodeDiagnosticsFeedbackLoopPanel'
describe('CodeDiagnosticsFeedbackLoopPanel', () => { it('shows bounded Core diagnostics actions', () => { render(<CodeDiagnosticsFeedbackLoopPanel connection="disconnected" />); expect(screen.getByRole('region', { name: 'Code Diagnostics Feedback Loop' })).toBeTruthy(); expect(screen.getByRole('button', { name: 'delta' })).toBeTruthy() }) })
