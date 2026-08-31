/** @vitest-environment jsdom */

import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'

import type { CoreEvent } from '../src/shared/api'
import { AdaptiveToolCatalogPanel } from '../src/renderer/src/AdaptiveToolCatalogPanel'

afterEach(cleanup)

describe('AdaptiveToolCatalogPanel', () => {
  it('renders only the bounded Core model loadout', () => {
    const event: CoreEvent = {
      sequenceId: 1,
      taskId: 'task',
      eventType: 'model.context',
      payload: JSON.stringify({ tools: ['filesystem.read', 'git.status', 'secret.internal'] }),
      executionEvent: null
    }
    render(<AdaptiveToolCatalogPanel connection="connected" events={[event]} />)
    expect(screen.getByText('filesystem.read')).toBeTruthy()
    expect(screen.getByText('git.status')).toBeTruthy()
    expect(screen.getByText('secret.internal')).toBeTruthy()
    expect(screen.getByText(/Полные schemas не попадают/)).toBeTruthy()
  })
})
