import { FrameError, LENGTH_PREFIX_BYTES, MAX_FRAME_BYTES, readLength } from './frame-codec'

/**
 * Incremental reader over a byte stream that never buffers more than one
 * oversized frame: an announced length above MAX_FRAME_BYTES fails immediately
 * instead of accumulating attacker-controlled bytes.
 */
export class FrameReader {
  private buffered: Uint8Array = new Uint8Array(0)
  private failed = false

  /** Appends a chunk and returns every complete frame payload it produced. */
  push(chunk: Uint8Array): Uint8Array[] {
    if (this.failed) {
      throw new FrameError('too-large')
    }
    this.buffered = concat(this.buffered, chunk)

    const frames: Uint8Array[] = []
    for (;;) {
      if (this.buffered.byteLength < LENGTH_PREFIX_BYTES) {
        return frames
      }
      const length = readLength(this.buffered)
      if (length > MAX_FRAME_BYTES) {
        this.failed = true
        this.buffered = new Uint8Array(0)
        throw new FrameError('too-large')
      }
      const total = LENGTH_PREFIX_BYTES + length
      if (this.buffered.byteLength < total) {
        return frames
      }
      frames.push(this.buffered.slice(LENGTH_PREFIX_BYTES, total))
      this.buffered = this.buffered.slice(total)
    }
  }

  /** Bytes held for an incomplete frame; used by backpressure diagnostics. */
  get pendingBytes(): number {
    return this.buffered.byteLength
  }

  reset(): void {
    this.buffered = new Uint8Array(0)
    this.failed = false
  }
}

function concat(left: Uint8Array, right: Uint8Array): Uint8Array {
  if (left.byteLength === 0) {
    return right.slice()
  }
  const merged = new Uint8Array(left.byteLength + right.byteLength)
  merged.set(left, 0)
  merged.set(right, left.byteLength)
  return merged
}
