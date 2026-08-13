import { describe, expect, it } from 'vitest'

import {
  REDACTED,
  REDACTED_PATH,
  isSensitiveName,
  redactArgv,
  redactError,
  redactText,
  redactValue
} from '../src/main/diagnostics/redact'

describe('redaction layer', () => {
  it('redacts known credential shapes in free text', () => {
    expect(redactText('token sk-abcdefghijklmnop')).toBe(`token ${REDACTED}`)
    expect(redactText('Authorization: Bearer abc.def')).toContain(REDACTED)
    expect(redactText('ghp_0123456789abcdef')).toBe(REDACTED)
    expect(redactText('write to user@example.com')).toContain(REDACTED)
  })

  it('redacts filesystem and pipe paths', () => {
    expect(redactText('opened C:\\Users\\hime\\workspace\\secret.txt')).toBe(
      `opened ${REDACTED_PATH}`
    )
    expect(redactText('pipe \\\\.\\pipe\\evohime-core-v1')).toContain(REDACTED_PATH)
  })

  it('drops values of sensitive keys and keeps ordinary ones', () => {
    expect(isSensitiveName('provider_api_key')).toBe(true)
    expect(
      redactValue({ apiKey: 'plaintext', taskId: 'task-1', nested: { password: 'x' } })
    ).toEqual({
      apiKey: REDACTED,
      taskId: 'task-1',
      nested: { password: REDACTED }
    })
  })

  it('keeps only the error class and message, never the stack', () => {
    const error = new Error('failed opening C:\\Users\\hime\\core.jsonl')
    expect(redactError(error)).toBe(`Error: failed opening ${REDACTED_PATH}`)
    expect(redactError(error)).not.toContain('at ')
  })

  it('keeps flag names but never argument values', () => {
    expect(redactArgv(['--pipe=\\\\.\\pipe\\evohime', 'C:\\path', '--flag'])).toEqual([
      `--pipe=${REDACTED}`,
      REDACTED,
      '--flag'
    ])
  })

  it('bounds long text', () => {
    expect(redactText('a'.repeat(5_000)).length).toBeLessThanOrEqual(2_001)
  })
})
