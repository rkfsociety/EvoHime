/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'

import type { CoreEvent, SkillCatalog } from '../src/shared/api'
import { SkillCatalogPanel } from '../src/renderer/src/SkillCatalogPanel'

const catalog: SkillCatalog = {
  schemaVersion: 1,
  skills: [{
    schemaVersion: 1,
    skillId: 'reviewer',
    name: 'Reviewer',
    description: 'Bounded review instructions',
    version: '1.0.0',
    scope: 'project',
    sourceKind: 'project_native',
    sourceRef: '.agents/skills/reviewer/SKILL.md',
    contentHash: 'hash-reviewer',
    allowedTools: ['workspace.read'],
    requiredCapabilities: ['workspace.read'],
    disableModelInvocation: true,
    referenceCount: 0,
    validationStatus: 'valid',
    validationErrorCode: '',
    warnings: []
  }],
  diagnostics: []
}

function installApi() {
  const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
  ;(window as unknown as { evohime: unknown }).evohime = {
    v1: { apiVersion: 1, invoke, subscribe: () => () => {}, writeClipboardText: async () => true }
  }
  return invoke
}

afterEach(() => {
  cleanup()
  delete (window as unknown as { evohime?: unknown }).evohime
})

describe('SkillCatalogPanel', () => {
  it('requests metadata and loads SKILL.md only after an explicit action', async () => {
    const invoke = installApi()
    const event: CoreEvent = {
      sequenceId: 1,
      taskId: '',
      eventType: 'skills.catalog',
      payload: '',
      executionEvent: null,
      taskCheckpoint: null,
      skillCatalog: catalog
    }

    render(<SkillCatalogPanel connection="connected" events={[event]} workspace={'C:\\workspace'} />)
    expect(await vi.waitFor(() => invoke.mock.calls.some(([command]) => command === 'core.listSkills'))).toBe(true)
    expect(screen.getByText('Reviewer')).toBeTruthy()
    expect(screen.queryByRole('textbox')).toBeNull()
    expect(screen.queryByText('loaded body')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Загрузить skill явно' }))
    expect(invoke.mock.calls.find(([command]) => command === 'core.loadSkill')?.[1]).toEqual({
      workspacePath: 'C:\\workspace', skillId: 'reviewer', maxBytes: 256 * 1024
    })
  })
})
