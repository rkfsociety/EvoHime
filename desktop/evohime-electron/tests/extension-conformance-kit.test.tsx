/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ExtensionConformanceKitPanel } from '../src/renderer/src/ExtensionConformanceKitPanel'
vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))
describe('extension conformance kit panel', () => { it('exposes bounded conformance actions', () => { render(<ExtensionConformanceKitPanel connection="disconnected" events={[]} />); expect(screen.getByRole('region', { name: 'Extension Conformance Kit' })).toBeTruthy(); expect(screen.getByRole('button', { name: 'run' })).toBeTruthy() }) })
