import { FrameError, LENGTH_PREFIX_BYTES, MAX_FRAME_BYTES, readLength } from './frame-codec'

/**
 * Incremental reader over a byte stream that never buffers more than one
 * oversized frame: an announced length above MAX_FRAME_BYTES fails immediately
 * instead of accumulating attacker-controlled bytes.
 */
export class FrameReader {
  private readonly header = new Uint8Array(LENGTH_PREFIX_BYTES)
  private headerBytes = 0
  private payload: Uint8Array | undefined
  private payloadBytes = 0
  private failed = false

  /** Appends a chunk and returns every complete frame payload it produced. */
  push(chunk: Uint8Array): Uint8Array[] {
    if (this.failed) {
      throw new FrameError('too-large')
    }

    const frames: Uint8Array[] = []
    let offset = 0
    while (offset < chunk.byteLength) {
      if (this.payload === undefined) {
        const headerBytes = Math.min(
          LENGTH_PREFIX_BYTES - this.headerBytes,
          chunk.byteLength - offset
        )
        this.header.set(chunk.subarray(offset, offset + headerBytes), this.headerBytes)
        this.headerBytes += headerBytes
        offset += headerBytes

        if (this.headerBytes < LENGTH_PREFIX_BYTES) {
          return frames
        }

        const length = readLength(this.header)
        this.headerBytes = 0
        if (length > MAX_FRAME_BYTES) {
          this.failed = true
          this.payload = undefined
          this.payloadBytes = 0
          throw new FrameError('too-large')
        }
        this.payload = new Uint8Array(length)
        this.payloadBytes = 0
        if (length === 0) {
          frames.push(this.payload)
          this.payload = undefined
          continue
        }
      }

      const payload = this.payload
      const payloadBytes = Math.min(
        payload.byteLength - this.payloadBytes,
        chunk.byteLength - offset
      )
      payload.set(chunk.subarray(offset, offset + payloadBytes), this.payloadBytes)
      this.payloadBytes += payloadBytes
      offset += payloadBytes

      if (this.payloadBytes < payload.byteLength) {
        return frames
      }

      frames.push(payload)
      this.payload = undefined
      this.payloadBytes = 0
    }

    return frames
  }

  /** Bytes held for an incomplete frame; used by backpressure diagnostics. */
  get pendingBytes(): number {
    return (
      this.headerBytes +
      (this.payload === undefined ? 0 : LENGTH_PREFIX_BYTES + this.payloadBytes)
    )
  }

  reset(): void {
    this.headerBytes = 0
    this.payload = undefined
    this.payloadBytes = 0
    this.failed = false
  }
}
