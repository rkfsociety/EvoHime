import { createHash, generateKeyPairSync, sign } from 'node:crypto'
import { describe, expect, it } from 'vitest'
import { verifyRuntimeEd25519 } from '../src/main/receipt-crypto'
import { payloadBytes, type ReceiptEnvelope } from '../src/shared/receipt-contract'

describe('runtime receipt signatures', () => {
  it('verifies a signature over the payload SHA-256 digest', () => {
    const { privateKey, publicKey } = generateKeyPairSync('ed25519')
    const payload = {
      receipt_version: 1,
      receipt_id: '018f0f2a-1111-7111-8111-111111111111',
      action_id: '018f0f2a-2222-7222-8222-222222222222',
      receipt_kind: 'pre_action',
      action_status: 'prepared',
      timestamp: '2026-08-17T10:00:00.000Z',
      task_id: 'task-1',
      run_id: 'run-1',
      tool_name: 'filesystem.write',
      tool_args_hash: 'a'.repeat(64),
      policy_id: 'policy-v1',
      policy_decision: 'allow',
    }
    const digest = createHash('sha256').update(Buffer.from(payloadBytes(payload))).digest()
    const signature = sign(null, digest, privateKey).toString('base64url')
    const rawPublic = publicKey.export({ format: 'der', type: 'spki' }).subarray(-32)
    const envelope: ReceiptEnvelope = { payload, key_id: 'key-1', signature_algorithm: 'Ed25519', signature }
    expect(verifyRuntimeEd25519(envelope, rawPublic)).toBe(true)
  })
})
