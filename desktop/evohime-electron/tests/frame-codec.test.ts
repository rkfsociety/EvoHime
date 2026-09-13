import { describe, expect, it } from 'vitest'

import {
  decodeFrame,
  encodeFrame,
  FrameError,
  MAX_FRAME_BYTES
} from '../src/main/ipc/frame-codec'
import { FrameReader } from '../src/main/ipc/frame-reader'

const text = (value: string): Uint8Array => new TextEncoder().encode(value)

describe('frame codec', () => {
  it('round-trips a bounded frame', () => {
    expect(decodeFrame(encodeFrame(text('hello')))).toEqual(text('hello'))
  })

  it('rejects an oversized payload', () => {
    expect(() => encodeFrame(new Uint8Array(MAX_FRAME_BYTES + 1))).toThrow(FrameError)
  })

  it('rejects a truncated frame', () => {
    expect(() => decodeFrame(new Uint8Array([1, 2, 3]))).toThrow(FrameError)
  })

  it('rejects trailing bytes', () => {
    const frame = encodeFrame(text('hello'))
    const padded = new Uint8Array(frame.byteLength + 1)
    padded.set(frame)
    expect(() => decodeFrame(padded)).toThrow(FrameError)
  })
})

describe('frame reader', () => {
  it('reassembles frames split across chunks', () => {
    const reader = new FrameReader()
    const frame = encodeFrame(text('streamed'))
    expect(reader.push(frame.subarray(0, 3))).toEqual([])
    expect(reader.pendingBytes).toBe(3)
    expect(reader.push(frame.subarray(3, 6))).toEqual([])
    expect(reader.pendingBytes).toBe(6)
    expect(reader.push(frame.subarray(6))).toEqual([text('streamed')])
    expect(reader.pendingBytes).toBe(0)
  })

  it('returns several frames from one chunk', () => {
    const reader = new FrameReader()
    const first = encodeFrame(text('one'))
    const second = encodeFrame(text('two'))
    const third = encodeFrame(text('three'))
    const chunk = new Uint8Array(
      first.byteLength + second.byteLength + third.byteLength
    )
    chunk.set(first)
    chunk.set(second, first.byteLength)
    chunk.set(third, first.byteLength + second.byteLength)
    expect(reader.push(chunk)).toEqual([
      text('one'),
      text('two'),
      text('three')
    ])
    expect(reader.pendingBytes).toBe(0)
  })

  it('returns an empty frame when its header is the final input', () => {
    const reader = new FrameReader()

    expect(reader.push(encodeFrame(new Uint8Array(0)))).toEqual([
      new Uint8Array(0)
    ])
    expect(reader.pendingBytes).toBe(0)
  })

  it('reassembles a maximum-size frame from highly fragmented chunks', () => {
    const reader = new FrameReader()
    const payload = new Uint8Array(MAX_FRAME_BYTES)
    for (let offset = 0; offset < payload.byteLength; offset += 4096) {
      payload[offset] = (offset / 4096) % 251
    }
    const frame = encodeFrame(payload)
    const frames: Uint8Array[] = []

    for (let offset = 0; offset < frame.byteLength; offset += 4096) {
      frames.push(...reader.push(frame.subarray(offset, offset + 4096)))
    }

    expect(frames).toHaveLength(1)
    expect(frames[0]?.byteLength).toBe(payload.byteLength)
    expect(frames[0]?.[payload.byteLength - 1]).toBe(
      payload[payload.byteLength - 1]
    )
    let matches = true
    for (let offset = 0; offset < payload.byteLength; offset += 4096) {
      if (frames[0]?.[offset] !== payload[offset]) {
        matches = false
        break
      }
    }
    expect(matches).toBe(true)
    expect(reader.pendingBytes).toBe(0)
  })

  it('fails fast on an announced length above the frame limit', () => {
    const reader = new FrameReader()
    const header = new Uint8Array(4)
    new DataView(header.buffer).setUint32(0, MAX_FRAME_BYTES + 1, true)
    expect(() => reader.push(header)).toThrow(FrameError)
    // The oversized announcement is not buffered while waiting for more bytes.
    expect(reader.pendingBytes).toBe(0)
  })
})
