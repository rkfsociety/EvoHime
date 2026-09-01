// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { RevisionSafeWorkspaceFilesPanel } from '../src/renderer/src/RevisionSafeWorkspaceFilesPanel'

vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))

describe('revision-safe workspace files panel', () => {
  it('exposes logical path and precondition without host path controls', () => {
    render(<RevisionSafeWorkspaceFilesPanel connection="disconnected" events={[]} />)
    expect(screen.getByLabelText('Logical path')).toBeTruthy()
    expect(screen.getByLabelText('Expected SHA-256 hash')).toBeTruthy()
    expect(screen.queryByLabelText(/workspace path/i)).toBeNull()
  })
})
