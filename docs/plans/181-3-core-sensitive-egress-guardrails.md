# План 181.3 — Approvals, recovery и projections

## Изменить

1. Create `RequiresApproval` through the existing approval owner with only
   categories/counts/reason metadata. Bind approval to exact payload, route,
   policy and detector hashes; any change invalidates it.
2. Persist only request IDs, hashes, detector/policy refs, entity kinds/counts/
   confidence, privacy, route/profile refs, disposition, approval ref and reason
   codes. Do not persist raw values/spans/context or reversible mappings.
3. Make classification/transform deterministic and replayable from an
   authoritative snapshot; after restart missing payload, stale policy/pack or
   mismatched hashes remains blocked/re-evaluated.
4. Preserve existing provider unknown-outcome/no-blind-retry semantics; guardrail
   never repeats an external effect and never lets renderer lower disposition.
5. Add bounded IPC/diagnostics projections for categories/counts/hashes/status
   (`detected`, `transformed`, `approval_required`, `blocked`, `unavailable`,
   `payload_changed`) without raw text or credentials.

## Зависимости

### Блокирующие

- Plan 181.1–181.2, existing approval owner, execution/provenance/recovery and
  authenticated IPC/diagnostics policy.

### Опциональные

- Existing operations UI may show metadata; no new DLP-specific top-level tab
  is required.

## Проверка

Test restart awaiting approval, stale policy/detector re-evaluation, missing
snapshot block, ambiguous provider outcome, concurrent requests, redactor carry
isolation and no raw PII in SQLite/logs/errors/IPC. Renderer mutation and
external sanitizer/network-call attempts must be rejected structurally.

## Rollback и evidence

Approval or projection failure leaves external dispatch blocked and does not
discard historical metadata. A conservative v1 policy remains available during
rollout; no bypass flag is added for recovery.
