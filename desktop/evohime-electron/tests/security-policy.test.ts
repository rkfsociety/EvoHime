import { describe, expect, it } from 'vitest'

import { DEFAULT_CORE_PIPE_NAME, normalizePipeName, readLaunchContext } from '../src/main/ipc/launch-context'
import { ReloadLimiter } from '../src/main/recovery'
import {
  CONTENT_SECURITY_POLICY,
  findForbiddenSwitches,
  isAllowedExternalUrl
} from '../src/main/security-policy'

describe('production security policy', () => {
  it('detects debug switches regardless of prefix or value', () => {
    expect(
      findForbiddenSwitches([
        '--remote-debugging-port=9222',
        '-inspect',
        '--no-sandbox',
        '--safe-flag'
      ])
    ).toEqual(['--remote-debugging-port=9222', '-inspect', '--no-sandbox'])
  })

  it('allows only https URLs inside the allow-listed origins', () => {
    expect(isAllowedExternalUrl('https://github.com/evohime')).toBe(true)
    expect(isAllowedExternalUrl('https://github.com.evil.tld/evohime')).toBe(false)
    expect(isAllowedExternalUrl('https://evil.tld/?next=https://github.com/')).toBe(false)
    expect(isAllowedExternalUrl('http://github.com/')).toBe(false)
    expect(isAllowedExternalUrl('file:///C:/Windows/System32/cmd.exe')).toBe(false)
    expect(isAllowedExternalUrl('javascript:alert(1)')).toBe(false)
    expect(isAllowedExternalUrl('not a url')).toBe(false)
  })

  it('keeps the production CSP free of eval, inline script and remote content', () => {
    expect(CONTENT_SECURITY_POLICY).toContain("default-src 'self'")
    expect(CONTENT_SECURITY_POLICY).not.toContain('unsafe-eval')
    expect(CONTENT_SECURITY_POLICY).not.toContain('unsafe-inline')
    expect(CONTENT_SECURITY_POLICY).not.toContain('http')
  })
})

describe('launch contract', () => {
  it('falls back to the developer pipe when the supervisor passed nothing', () => {
    const context = readLaunchContext({}, () => 'fixed-id')
    expect(context.pipeName).toBe(DEFAULT_CORE_PIPE_NAME)
    expect(context.developerLaunch).toBe(true)
    expect(context.challenge).toBe('')
  })

  it('takes pipe, challenge and liveness handle from the launcher environment', () => {
    const context = readLaunchContext(
      {
        EVOHIME_CORE_PIPE: 'evohime-core-abc123',
        EVOHIME_IPC_CHALLENGE: 'challenge-value',
        EVOHIME_SUPERVISOR_LIVENESS_EVENT: 'evohime-liveness',
        EVOHIME_CLIENT_ID: 'shell-1',
        EVOHIME_SESSION_ID: 'session-1'
      },
      () => 'fixed-id'
    )
    expect(context.pipeName).toBe('\\\\.\\pipe\\evohime-core-abc123')
    expect(context.developerLaunch).toBe(false)
    expect(context.clientId).toBe('shell-1')
  })

  it('rejects remote or malformed pipe names', () => {
    expect(normalizePipeName('\\\\attacker\\pipe\\evohime')).toBeNull()
    expect(normalizePipeName('..\\..\\evil')).toBeNull()
    expect(normalizePipeName('')).toBeNull()
    expect(normalizePipeName('a'.repeat(300))).toBeNull()
  })
})

describe('renderer reload budget', () => {
  it('stops reloading after the threshold inside the window', () => {
    let now = 0
    const limiter = new ReloadLimiter(2, 1_000, () => now)
    expect(limiter.record()).toBe('reload')
    expect(limiter.record()).toBe('reload')
    expect(limiter.record()).toBe('recovery-window')
  })

  it('forgets failures older than the window', () => {
    let now = 0
    const limiter = new ReloadLimiter(2, 1_000, () => now)
    limiter.record()
    limiter.record()
    now = 2_000
    expect(limiter.record()).toBe('reload')
  })
})
