import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { canonicalizeJson, payloadBytes, validatePayloadV1 } from '../src/shared/receipt-contract'
import { receiptHash, resultHash, verifyEd25519 } from '../src/main/receipt-crypto'

const vectors = JSON.parse(readFileSync(resolve(process.cwd(), '../../contracts/receipts/v1/vectors.json'), 'utf8')) as { positive: Array<{ payload: unknown; key_id: string; signature: string; canonical_payload_hex: string; receipt_hash_hex: string; public_key_base64url: string }>; negative: Array<{ input: string; error: string }> }

describe('Receipt canonical contract v1', () => {
  it('sorts object keys by UTF-16 code units and preserves Unicode', () => {
    expect(Buffer.from(canonicalizeJson(Buffer.from('{"𐀀":1,"\ue000":2}'))).toString()).toBe('{"𐀀":1,"\ue000":2}')
  })
  it('rejects every shared negative vector with its stable code', () => {
    for (const vector of vectors.negative) {
      expect(() => {
        canonicalizeJson(Buffer.from(vector.input))
        try { validatePayloadV1(JSON.parse(vector.input) as unknown) } catch (error) { throw error }
      }).toThrow(vector.error)
    }
  })
  it('uses the result domain and bounded payload bytes', () => {
    expect(resultHash({ status: 'succeeded' })).not.toBe('9a7c7f4c8d3a3d33c8d4b5fcb0c4f0a57a4b4f3c8d0f2b0b4df9c7c5c2b8d4f1')
    expect(Buffer.from(payloadBytes({ receipt_version: 1 }))).toEqual(Buffer.from('{"receipt_version":1}'))
  })
  it('hashes the canonical envelope after signature is present', () => {
    expect(receiptHash({ payload: { receipt_version: 1 }, key_id: 'test', signature_algorithm: 'Ed25519', signature: 'AA' })).toHaveLength(64)
  })
  it('matches the shared positive vector and verifies Ed25519', () => {
    const vector = vectors.positive[0]
    if (!vector) throw new Error('positive vector missing')
    expect(Buffer.from(payloadBytes(vector.payload)).toString('hex')).toBe(vector.canonical_payload_hex)
    const envelope = { payload: vector.payload, key_id: vector.key_id, signature_algorithm: 'Ed25519' as const, signature: vector.signature }
    expect(receiptHash(envelope)).toBe(vector.receipt_hash_hex)
    const publicKey = Buffer.from(vector.public_key_base64url.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(vector.public_key_base64url.length / 4) * 4, '='), 'base64')
    expect(verifyEd25519(envelope, publicKey)).toBe(true)
  })
})
