// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { KnowledgeSourceRegistryProjectRolePanel } from '../src/renderer/src/KnowledgeSourceRegistryProjectRolePanel'
vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))
describe('Knowledge Source Registry panel', () => { it('keeps knowledge controls projection-only', () => { render(<KnowledgeSourceRegistryProjectRolePanel connection="starting" />); expect(screen.getByRole('region', { name: 'Knowledge Source Registry' })).toBeTruthy(); expect(screen.getByLabelText('Knowledge Source Registry JSON')).toBeTruthy() }) })
