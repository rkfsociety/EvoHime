import { describe, expect, it } from 'vitest'

import {
  MAX_CAPABILITIES,
  NegotiationError,
  negotiateProtocol
} from '../src/main/ipc/protocol-version'

describe('protocol negotiation', () => {
  it('accepts the same major and negotiates down to the lower minor', () => {
    const negotiated = negotiateProtocol({ major: 1, minor: 4 }, { major: 1, minor: 2 }, [], [])
    expect(negotiated.version).toEqual({ major: 1, minor: 2 })
  })

  it('intersects capabilities', () => {
    const negotiated = negotiateProtocol(
      { major: 1, minor: 0 },
      { major: 1, minor: 0 },
      ['replay', 'resync', 'future'],
      ['resync', 'replay']
    )
    expect(negotiated.capabilities).toEqual(['replay', 'resync'])
  })

  it('rejects a major mismatch', () => {
    expect(() => negotiateProtocol({ major: 1, minor: 0 }, { major: 2, minor: 0 }, [], [])).toThrow(
      NegotiationError
    )
  })

  it('rejects unbounded and malformed capability lists', () => {
    const tooMany = Array.from({ length: MAX_CAPABILITIES + 1 }, (_, index) => `cap-${index}`)
    expect(() =>
      negotiateProtocol({ major: 1, minor: 0 }, { major: 1, minor: 0 }, tooMany, [])
    ).toThrow(NegotiationError)
    expect(() =>
      negotiateProtocol({ major: 1, minor: 0 }, { major: 1, minor: 0 }, ['bad\ncap'], [])
    ).toThrow(NegotiationError)
    expect(() =>
      negotiateProtocol({ major: 1, minor: 0 }, { major: 1, minor: 0 }, [''], [])
    ).toThrow(NegotiationError)
  })
})
