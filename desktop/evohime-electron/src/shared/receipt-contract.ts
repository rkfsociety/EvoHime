export const RECEIPT_LIMITS = {
  maxEnvelopeBytes: 8192,
  maxPayloadBytes: 4096,
  maxIdentifierBytes: 128,
  maxDepth: 4,
  fingerprintInputVersion: 1,
  samplingPolicyVersion: 1,
  defaultReadOnlySamplingRate: 10,
  maxPreviewBytes: 1024,
} as const

export type ReceiptErrorCode =
  | 'receipt.too_large' | 'receipt.payload_too_large' | 'receipt.invalid_utf8'
  | 'receipt.invalid_json' | 'receipt.duplicate_key' | 'receipt.unsupported_version'
  | 'receipt.schema_violation' | 'receipt.secret_field' | 'receipt.non_canonical'
  | 'receipt.chain_incomplete' | 'receipt.key_unknown' | 'receipt.timestamp_skew'
  | 'receipt.signature_invalid' | 'receipt.hash_mismatch'

export interface ReceiptEnvelope { payload: unknown; key_id: string; signature_algorithm: 'Ed25519'; signature: string }

export function canonicalizeJson(input: Uint8Array): Uint8Array {
  let text: string
  try { text = new TextDecoder('utf-8', { fatal: true }).decode(input) } catch { throw new Error('receipt.invalid_utf8') }
  let value: unknown
  try { value = JSON.parse(text, (_key, item) => item) } catch { throw new Error('receipt.invalid_json') }
  if (hasDuplicateKeys(text)) throw new Error('receipt.duplicate_key')
  rejectLoneSurrogates(value)
  const result = writeJcs(value, 0)
  return new TextEncoder().encode(result)
}

function rejectLoneSurrogates(value: unknown): void {
  if (typeof value === 'string') {
    for (let index = 0; index < value.length; index++) {
      const code = value.charCodeAt(index)
      if (code >= 0xd800 && code <= 0xdbff) { const next = value.charCodeAt(index + 1); if (next < 0xdc00 || next > 0xdfff) throw new Error('receipt.schema_violation'); index++ }
      else if (code >= 0xdc00 && code <= 0xdfff) throw new Error('receipt.schema_violation')
    }
  } else if (Array.isArray(value)) value.forEach(rejectLoneSurrogates)
  else if (value !== null && typeof value === 'object') Object.values(value).forEach(rejectLoneSurrogates)
}

function writeJcs(value: unknown, depth: number): string {
  if (depth > RECEIPT_LIMITS.maxDepth) throw new Error('receipt.schema_violation')
  if (value === null || typeof value === 'boolean' || typeof value === 'number') return JSON.stringify(value)
  if (typeof value === 'string') return JSON.stringify(value)
  if (Array.isArray(value)) return `[${value.map((item) => writeJcs(item, depth + 1)).join(',')}]`
  if (typeof value === 'object') {
    const entries = Object.entries(value).sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0)
    return `{${entries.map(([key, item]) => `${JSON.stringify(key)}:${writeJcs(item, depth + 1)}`).join(',')}}`
  }
  throw new Error('receipt.invalid_json')
}

function hasDuplicateKeys(input: string): boolean {
  const stack: Array<Set<string>> = []
  let index = 0
  while (index < input.length) {
    const char = input[index]
    if (char === '"') {
      const start = index++
      while (index < input.length) { if (input[index] === '\\') index += 2; else if (input[index++] === '"') break }
      let next = index
      while (/\s/.test(input[next] ?? '')) next++
      if (input[next] === ':') {
        const key = JSON.parse(input.slice(start, index)) as string
        const current = stack.at(-1)
        if (current?.has(key)) return true
        current?.add(key)
      }
    } else if (char === '{') { stack.push(new Set()); index++ }
    else if (char === '}') { stack.pop(); index++ }
    else index++
  }
  return false
}

export function payloadBytes(payload: unknown): Uint8Array {
  const bytes = canonicalizeJson(new TextEncoder().encode(JSON.stringify(payload)))
  if (bytes.length > RECEIPT_LIMITS.maxPayloadBytes) throw new Error('receipt.payload_too_large')
  return bytes
}

export function validateTypedIdentifier(value: string): boolean {
  const bytes = new TextEncoder().encode(value).length
  return bytes >= 1 && bytes <= RECEIPT_LIMITS.maxIdentifierBytes && /^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(value)
}

export function validatePayloadV1(payload: unknown): void {
  if (payload === null || typeof payload !== 'object' || Array.isArray(payload)) throw new Error('receipt.schema_violation')
  const record = payload as Record<string, unknown>
  if (record.receipt_version !== 1) throw new Error('receipt.unsupported_version')
  for (const key of Object.keys(record)) {
    const lower = key.toLowerCase()
    if (['secret', 'token', 'password', 'api_key', 'apikey', 'authorization', 'cookie', 'private_key'].some((part) => lower.includes(part))) throw new Error('receipt.secret_field')
  }
  const required = ['receipt_id', 'action_id', 'receipt_kind', 'action_status', 'timestamp', 'task_id', 'run_id']
  if (required.some((key) => !(key in record))) throw new Error('receipt.schema_violation')
  for (const key of ['receipt_id', 'action_id']) {
    const value = record[key]
    if (typeof value !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value)) throw new Error('receipt.schema_violation')
  }
  if (typeof record.timestamp !== 'string' || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(record.timestamp)) throw new Error('receipt.schema_violation')
  if (record.receipt_kind === 'pre_action' && record.action_status !== 'prepared') throw new Error('receipt.schema_violation')
  if (record.receipt_kind === 'post_action' && !['succeeded', 'failed', 'cancelled'].includes(String(record.action_status))) throw new Error('receipt.schema_violation')
  if (record.receipt_kind === 'refusal' && record.action_status !== 'refused') throw new Error('receipt.schema_violation')
}
