/**
 * Wire framing for `desktop-ipc-v1`.
 *
 * Mirrors `crates/desktop-ipc/src/lib.rs`: a little-endian `u32` length prefix
 * followed by exactly that many protobuf bytes, bounded by MAX_FRAME_BYTES.
 */

export const MAX_FRAME_BYTES = 4 * 1024 * 1024
export const LENGTH_PREFIX_BYTES = 4

export type FrameErrorKind = 'truncated' | 'too-large' | 'length-mismatch'

export class FrameError extends Error {
  constructor(readonly kind: FrameErrorKind) {
    super(`ipc frame error: ${kind}`)
    this.name = 'FrameError'
  }
}

export function encodeFrame(payload: Uint8Array): Uint8Array {
  if (payload.byteLength > MAX_FRAME_BYTES) {
    throw new FrameError('too-large')
  }
  const frame = new Uint8Array(LENGTH_PREFIX_BYTES + payload.byteLength)
  new DataView(frame.buffer).setUint32(0, payload.byteLength, true)
  frame.set(payload, LENGTH_PREFIX_BYTES)
  return frame
}

export function decodeFrame(frame: Uint8Array): Uint8Array {
  if (frame.byteLength < LENGTH_PREFIX_BYTES) {
    throw new FrameError('truncated')
  }
  const length = readLength(frame)
  if (length > MAX_FRAME_BYTES) {
    throw new FrameError('too-large')
  }
  if (frame.byteLength < LENGTH_PREFIX_BYTES + length) {
    throw new FrameError('truncated')
  }
  if (frame.byteLength !== LENGTH_PREFIX_BYTES + length) {
    throw new FrameError('length-mismatch')
  }
  return frame.subarray(LENGTH_PREFIX_BYTES)
}

export function readLength(frame: Uint8Array): number {
  return new DataView(frame.buffer, frame.byteOffset, frame.byteLength).getUint32(0, true)
}
