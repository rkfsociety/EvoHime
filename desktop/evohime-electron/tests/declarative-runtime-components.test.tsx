/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { DeclarativeRuntimeComponentsPanel } from '../src/renderer/src/DeclarativeRuntimeComponentsPanel'
vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))
describe('declarative runtime components panel', () => { it('exposes bounded Core actions', () => { render(<DeclarativeRuntimeComponentsPanel connection="disconnected" events={[]} />); expect(screen.getByRole('region', { name: 'Declarative Runtime Components' })).toBeTruthy(); expect(screen.getByRole('button', { name: 'rehydrate' })).toBeTruthy() }) })
