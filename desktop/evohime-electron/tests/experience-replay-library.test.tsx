// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { ExperienceReplayLibraryPanel } from '../src/renderer/src/ExperienceReplayLibraryPanel'
describe('ExperienceReplayLibraryPanel', () => { it('shows bounded untrusted experience actions', () => { render(<ExperienceReplayLibraryPanel connection="disconnected" events={[]} />); expect(screen.getByRole('region', { name: 'Experience Replay Library' })).toBeTruthy(); expect(screen.getByText(/untrusted advice/i)).toBeTruthy(); expect(screen.getByRole('button', { name: 'context' })).toBeTruthy() }) })
