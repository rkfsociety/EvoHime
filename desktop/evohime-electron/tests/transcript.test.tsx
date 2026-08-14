import { describe, expect, it } from 'vitest'

import type { CoreEvent } from '../src/shared/api'
import { buildTranscript } from '../src/renderer/src/transcript'

/**
 * The transcript is what turns a raw event stream into a readable chat. These
 * tests pin the collapsing rules: one message per answer, one line per stretch
 * of tool work, and no bookkeeping events leaking into the conversation.
 */

let sequence = 0

/** Core serialises CoreEvent as an externally tagged enum. */
function event(variant: string, fields: Record<string, unknown>): CoreEvent {
  sequence += 1
  const eventType = {
    TaskStarted: 'task.started',
    ModelContext: 'model.context',
    AssistantDelta: 'agent.message.delta',
    ToolStarted: 'tool.started',
    ToolOutput: 'tool.output',
    ApprovalRequired: 'approval.required',
    TaskCompleted: 'task.completed',
    TaskFailed: 'task.failed',
    TaskStopped: 'task.stopped'
  }[variant]
  return {
    sequenceId: sequence,
    taskId: 'task-1',
    eventType: eventType ?? variant,
    payload: JSON.stringify({ [variant]: { task_id: 'task-1', ...fields } })
  }
}

/** The shell keeps events newest-first. */
function stream(...events: CoreEvent[]): CoreEvent[] {
  return [...events].reverse()
}

describe('transcript', () => {
  it('merges answer deltas into one message', () => {
    const { entries } = buildTranscript(
      stream(
        event('AssistantDelta', { content: 'Смотрю ' }),
        event('AssistantDelta', { content: 'структуру ' }),
        event('AssistantDelta', { content: 'проекта.' })
      )
    )

    expect(entries).toHaveLength(1)
    expect(entries[0]).toMatchObject({ kind: 'agent', text: 'Смотрю структуру проекта.' })
  })

  it('collapses a run of tool calls into one activity line', () => {
    const { entries } = buildTranscript(
      stream(
        event('ToolStarted', { tool_name: 'filesystem.list' }),
        event('ToolOutput', { tool_name: 'filesystem.list', output: 'src\nCargo.toml' }),
        event('ToolStarted', { tool_name: 'filesystem.read' }),
        event('ToolOutput', { tool_name: 'filesystem.read', output: '# EvoHime' })
      )
    )

    expect(entries).toHaveLength(1)
    expect(entries[0]).toMatchObject({ kind: 'activity', running: false })
    const activity = entries[0] as Extract<(typeof entries)[number], { kind: 'activity' }>
    expect(activity.calls).toEqual([
      { tool: 'filesystem.list', output: 'src\nCargo.toml', running: false },
      { tool: 'filesystem.read', output: '# EvoHime', running: false }
    ])
  })

  it('starts a new activity line after the model speaks', () => {
    const { entries } = buildTranscript(
      stream(
        event('ToolStarted', { tool_name: 'filesystem.list' }),
        event('ToolOutput', { tool_name: 'filesystem.list', output: '.' }),
        event('AssistantDelta', { content: 'Вижу Rust-проект.' }),
        event('ToolStarted', { tool_name: 'filesystem.read' })
      )
    )

    expect(entries.map((entry) => entry.kind)).toEqual(['activity', 'agent', 'activity'])
    expect(entries.at(-1)).toMatchObject({ kind: 'activity', running: true })
  })

  it('keeps a running call visible until its output arrives', () => {
    const { entries, finished } = buildTranscript(
      stream(event('ToolStarted', { tool_name: 'shell.execute' }))
    )

    expect(finished).toBe(false)
    expect(entries[0]).toMatchObject({ kind: 'activity', running: true })
  })

  it('drops bookkeeping the user has no use for', () => {
    const { entries } = buildTranscript(
      stream(
        event('TaskStarted', { prompt: 'Изучи проект' }),
        event('ModelContext', { model: 'x', system_prompt: 'y', user_prompt: 'z' })
      )
    )

    expect(entries).toEqual([])
  })

  it('reports a failure as a readable message', () => {
    const { entries, finished } = buildTranscript(
      stream(event('TaskFailed', { error: 'model request failed: 403 Forbidden' }))
    )

    expect(finished).toBe(true)
    expect(entries[0]).toMatchObject({
      kind: 'result',
      failed: true,
      text: 'model request failed: 403 Forbidden'
    })
  })

  it('does not repeat an empty completion after the answer', () => {
    const { entries } = buildTranscript(
      stream(
        event('AssistantDelta', { content: 'Готово.' }),
        event('TaskCompleted', { final_message: '' })
      )
    )

    expect(entries).toHaveLength(1)
    expect(entries[0]).toMatchObject({ kind: 'agent', text: 'Готово.' })
  })

  it('surfaces a pending approval and clears it once the task ends', () => {
    const pending = buildTranscript(
      stream(
        event('ApprovalRequired', {
          approval_id: 'approval-1',
          tool_name: 'filesystem.write',
          permission: 'write',
          scope: 'src/main.rs'
        })
      )
    )
    expect(pending.approval).toMatchObject({ approvalId: 'approval-1', toolName: 'filesystem.write' })

    const done = buildTranscript(
      stream(
        event('ApprovalRequired', {
          approval_id: 'approval-1',
          tool_name: 'filesystem.write',
          permission: 'write',
          scope: 'src/main.rs'
        }),
        event('TaskCompleted', { final_message: 'Готово' })
      )
    )
    expect(done.approval).toBeNull()
  })

  it('keeps the structured approval preview for the safety UI', () => {
    const { approval } = buildTranscript(stream(event('ApprovalRequired', {
      approval_id: 'approval-2',
      tool_name: 'shell.execute',
      permission: 'ShellExecute',
      scope: 'crates',
      preview: {
        kind: 'command',
        summary: 'Запустить команду',
        command: 'cargo test -p evohime-core',
        cwd: 'crates',
        truncated: false
      }
    })))

    expect(approval).toMatchObject({
      approvalId: 'approval-2',
      preview: {
        kind: 'command',
        command: 'cargo test -p evohime-core',
        cwd: 'crates'
      }
    })
  })
})
