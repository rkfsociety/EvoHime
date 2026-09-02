// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { RuntimeInterventionPipelinePanel } from '../src/renderer/src/RuntimeInterventionPipelinePanel'
describe('RuntimeInterventionPipelinePanel', () => { it('shows Core intervention diagnostics action', () => { render(<RuntimeInterventionPipelinePanel connection="disconnected" events={[]} />); expect(screen.getByRole('region', { name: 'Runtime Intervention Pipeline' })).toBeTruthy(); expect(screen.getByRole('button', { name: 'Проверить intervention' })).toBeTruthy() }) })
