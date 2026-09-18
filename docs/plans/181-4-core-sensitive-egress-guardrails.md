# План 181.4 — Verification, release evidence и closure

## Изменить

1. Run focused detector, false-positive, routing/privacy, pre-dispatch,
   approval, recovery/concurrency, storage/log/IPC security tests and affected
   provider/tool/external-agent integration checks.
2. Verify no raw PII, spans, prompt body, credentials or reversible mapping in
   durable state, diagnostics, IPC or error messages; verify no remote sanitizer
   call and no weaker privacy fallback.
3. Update `docs/architecture.md`, `docs/current-state.md` and
   `docs/release-evidence.md` with actual detector pack/policy versions,
   destination semantics and explicit unsupported OCR/image/audio boundaries.
   Remove plan files only after all criteria are evidenced.

## Зависимости

### Блокирующие

- Completed 181.1–181.3 and fresh Rust/IPC/integration checks for every egress
  path claimed in the canonical docs.

### Опциональные

- Additional detector schemes and A2A/image-specific extensions remain future
  work and cannot weaken the verified text egress baseline.

## Проверка

Use narrow affected-crate tests first, then required CI/security evidence;
finish with `git diff --check`, status and a self-review for privacy, scope,
secrets and preserved user changes.

## Rollback и evidence

Keep plan active on any detector false-positive/security gap. Rollout can remain
conservative/blocked for an unsupported destination; it must not silently allow
external payloads or delete historical evidence.
