import { createHash, createPublicKey, verify } from 'node:crypto'
import { canonicalizeJson, payloadBytes, type ReceiptEnvelope, RECEIPT_LIMITS, validatePayloadV1 } from '../shared/receipt-contract'

export function sha256Hex(bytes: Uint8Array): string { return createHash('sha256').update(bytes).digest('hex') }

export function receiptHash(envelope: ReceiptEnvelope): string {
  const bytes = canonicalizeJson(Buffer.from(JSON.stringify(envelope), 'utf8'))
  if (bytes.length > RECEIPT_LIMITS.maxEnvelopeBytes) throw new Error('receipt.too_large')
  return sha256Hex(bytes)
}

export function resultHash(projection: unknown): string {
  const canonical = canonicalizeJson(Buffer.from(JSON.stringify(projection), 'utf8'))
  return sha256Hex(Buffer.concat([Buffer.from('evohime-result-v1\0', 'utf8'), canonical]))
}

export function verifyEd25519(envelope: ReceiptEnvelope, publicKeyRaw: Uint8Array): boolean {
  validatePayloadV1(envelope.payload)
  const key = createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), Buffer.from(publicKeyRaw)]), format: 'der', type: 'spki' })
  const signature = Buffer.from(envelope.signature.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(envelope.signature.length / 4) * 4, '='), 'base64')
  return verify(null, Buffer.from(payloadBytes(envelope.payload)), key, signature)
}
