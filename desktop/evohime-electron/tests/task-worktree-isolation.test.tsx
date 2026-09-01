// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { TaskWorktreeIsolationPanel } from '../src/renderer/src/TaskWorktreeIsolationPanel'

vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))

describe('task worktree isolation panel', () => {
  it('exposes only bounded refs and lifecycle actions', () => {
    render(<TaskWorktreeIsolationPanel connection="disconnected" events={[]} />)
    expect(screen.getByLabelText('Branch ref')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Создать registry' })).toBeTruthy()
    expect(screen.queryByLabelText(/host path/i)).toBeNull()
  })
})
