import type { ProtocolVersion } from '@shared/api'

/**
 * Version/capability negotiation, mirroring `negotiate_protocol` in
 * `crates/desktop-ipc/src/lib.rs`. A different major is always rejected; the
 * same major negotiates down to the lower minor and to the capability
 * intersection.
 */

export const LOCAL_PROTOCOL: ProtocolVersion = { major: 1, minor: 0 }

export const MAX_CAPABILITIES = 64
export const MAX_CAPABILITY_NAME_BYTES = 64

export type NegotiationErrorKind =
  | 'major-mismatch'
  | 'too-many-capabilities'
  | 'invalid-capability'

export class NegotiationError extends Error {
  constructor(readonly kind: NegotiationErrorKind) {
    super(`ipc negotiation error: ${kind}`)
    this.name = 'NegotiationError'
  }
}

export interface NegotiatedProtocol {
  readonly version: ProtocolVersion
  readonly capabilities: string[]
}

export function negotiateProtocol(
  local: ProtocolVersion,
  peer: ProtocolVersion,
  localCapabilities: readonly string[],
  peerCapabilities: readonly string[]
): NegotiatedProtocol {
  if (local.major !== peer.major) {
    throw new NegotiationError('major-mismatch')
  }
  const localCaps = normalizeCapabilities(localCapabilities)
  const peerCaps = new Set(normalizeCapabilities(peerCapabilities))
  return {
    version: { major: local.major, minor: Math.min(local.minor, peer.minor) },
    capabilities: localCaps.filter((capability) => peerCaps.has(capability))
  }
}

function normalizeCapabilities(values: readonly string[]): string[] {
  if (values.length > MAX_CAPABILITIES) {
    throw new NegotiationError('too-many-capabilities')
  }
  for (const capability of values) {
    const bytes = Buffer.byteLength(capability, 'utf8')
    if (bytes === 0 || bytes > MAX_CAPABILITY_NAME_BYTES || hasControlCharacter(capability)) {
      throw new NegotiationError('invalid-capability')
    }
  }
  return [...new Set(values)].sort()
}

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (code < 0x20 || code === 0x7f) {
      return true
    }
  }
  return false
}
