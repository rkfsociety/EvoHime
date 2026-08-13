import { describe, expect, it } from 'vitest'

import {
  DEFAULT_CORE_PIPE_NAME,
  handshakeProof,
  normalizePipeName,
  readLaunchContext
} from '../src/main/ipc/launch-context'
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
  const SECRET = 'ab'.repeat(32)
  const PIPE = '\\\\.\\pipe\\evohime-core-abc123'

  const missingFile = (): string => {
    throw new Error('no launch context')
  }

  const contextFile =
    (pipeName: string, secret: string) =>
    (): string =>
      JSON.stringify({ pipe_name: pipeName, secret })

  it('falls back to the developer pipe when the supervisor left no context', () => {
    const context = readLaunchContext({}, () => 'fixed-id', missingFile)
    expect(context.pipeName).toBe(DEFAULT_CORE_PIPE_NAME)
    expect(context.developerLaunch).toBe(true)
    expect(context.secret).toBe('')
    // Without a secret the shell sends an empty proof, and Core decides whether
    // an unauthenticated connection is acceptable.
    expect(handshakeProof(context, 'a'.repeat(64))).toBe('')
  })

  it('takes the pipe and secret from the protected launch context file', () => {
    const context = readLaunchContext(
      { EVOHIME_LAUNCH_CONTEXT: 'C:\\ctx\\session.json', EVOHIME_CLIENT_ID: 'shell-1' },
      () => 'fixed-id',
      contextFile(PIPE, SECRET)
    )
    expect(context.pipeName).toBe(PIPE)
    expect(context.secret).toBe(SECRET)
    expect(context.clientId).toBe('shell-1')
    expect(context.clientRole).toBe('shell')
    expect(context.developerLaunch).toBe(false)
  })

  it('takes the supervisor owner pid from the protected launch context', () => {
    const context = readLaunchContext(
      { EVOHIME_LAUNCH_CONTEXT: 'C:\\ctx\\session.json' },
      () => 'fixed-id',
      () => JSON.stringify({
        pipe_name: PIPE,
        secret: SECRET,
        supervisor_pid: 4242,
        supervisor_liveness_event: 'Local\\EvoHime.Supervisor.Liveness.fixed'
      })
    )
    expect(context.supervisorPid).toBe(4242)
    expect(context.livenessEvent).toContain('EvoHime.Supervisor.Liveness')
  })

  it('rejects an untrusted supervisor liveness event name', () => {
    const context = readLaunchContext(
      { EVOHIME_LAUNCH_CONTEXT: 'C:\\ctx\\session.json' },
      () => 'fixed-id',
      () => JSON.stringify({ pipe_name: PIPE, secret: SECRET, supervisor_liveness_event: 'Global\\attacker' })
    )
    expect(context.developerLaunch).toBe(true)
  })

  it('ignores a launch context with a remote pipe or a malformed secret', () => {
    const remotePipe = readLaunchContext(
      {},
      () => 'fixed-id',
      contextFile('\\\\attacker\\pipe\\evohime', SECRET)
    )
    expect(remotePipe.developerLaunch).toBe(true)

    const shortSecret = readLaunchContext({}, () => 'fixed-id', contextFile(PIPE, 'ab'))
    expect(shortSecret.developerLaunch).toBe(true)

    const malformedJson = readLaunchContext({}, () => 'fixed-id', () => 'not json')
    expect(malformedJson.developerLaunch).toBe(true)
  })

  it('binds the proof to the role, the client id and the nonce', () => {
    const nonce = 'ef'.repeat(32)
    const base = readLaunchContext({}, () => 'fixed-id', contextFile(PIPE, SECRET))
    const proof = handshakeProof(base, nonce)

    expect(proof).toHaveLength(64)
    expect(handshakeProof({ ...base, clientId: 'other' }, nonce)).not.toBe(proof)
    expect(handshakeProof({ ...base, clientRole: 'compatibility-shell' }, nonce)).not.toBe(proof)
    expect(handshakeProof(base, 'aa'.repeat(32))).not.toBe(proof)
    expect(handshakeProof({ ...base, secret: 'cd'.repeat(32) }, nonce)).not.toBe(proof)
  })

  it('derives the shared cross-implementation proof vector', () => {
    // The same vector is asserted by evohime_desktop_ipc::session and by the
    // WinUI compatibility shell, so all three stay wire-compatible.
    const context = readLaunchContext({}, () => 'fixed-id', contextFile(PIPE, SECRET))
    expect(
      handshakeProof(
        { ...context, clientRole: 'compatibility-shell', clientId: 'EvoHime.Desktop' },
        'cd'.repeat(32)
      )
    ).toBe('e7c7b06966269a86caf38e32d01ceccf5f1e9c52ab1e6646ac486c6e074941f3')
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
