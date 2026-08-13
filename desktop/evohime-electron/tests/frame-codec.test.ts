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
    expect(reader.push(frame.subarray(3, 6))).toEqual([])
    expect(reader.push(frame.subarray(6))).toEqual([text('streamed')])
    expect(reader.pendingBytes).toBe(0)
  })

  it('returns several frames from one chunk', () => {
    const reader = new FrameReader()
    const first = encodeFrame(text('one'))
    const second = encodeFrame(text('two'))
    const chunk = new Uint8Array(first.byteLength + second.byteLength)
    chunk.set(first)
    chunk.set(second, first.byteLength)
    expect(reader.push(chunk)).toEqual([text('one'), text('two')])
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
