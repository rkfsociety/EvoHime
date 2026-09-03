/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { PromptCachePlannerPanel } from '../src/renderer/src/PromptCachePlannerPanel'
vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))
describe('prompt cache planner panel', () => { it('exposes bounded planner actions', () => { render(<PromptCachePlannerPanel connection="disconnected" events={[]} />); expect(screen.getByRole('region', { name: 'Prompt Cache Planner' })).toBeTruthy(); expect(screen.getByRole('button', { name: 'plan' })).toBeTruthy(); }) })
