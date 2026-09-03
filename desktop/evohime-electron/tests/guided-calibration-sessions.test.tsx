/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { GuidedCalibrationSessionsPanel } from '../src/renderer/src/GuidedCalibrationSessionsPanel'
vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))
describe('guided calibration sessions panel', () => { it('exposes Core checked session actions', () => { render(<GuidedCalibrationSessionsPanel connection="disconnected" events={[]} />); expect(screen.getByRole('region', { name: 'Guided Calibration Sessions' })).toBeTruthy(); expect(screen.getByRole('button', { name: 'consolidate' })).toBeTruthy() }) })
