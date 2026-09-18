# План 179.4 — Verification, release evidence и closure

## Изменить

1. Run contract/storage/resolver/security/evaluation/recovery and IPC/Electron
   tests, including active-run revision retention and missing revision failure.
2. Verify raw production prompts, examples and hidden reasoning do not appear
   in registry, provenance, generic events, diagnostics or IPC projections.
3. Update canonical architecture/current-state/release evidence with actual
   strategy types, baseline behavior and unsupported paths; remove plan files
   only after closure is evidenced.

## Зависимости

### Блокирующие

- Completed 179.1–179.3 and fresh checks for affected crates, protocol and
  guided recipe integration.

### Опциональные

- Additional prompting techniques remain future profiles, not a reason to
  weaken the deterministic baseline.

## Проверка

Use focused Rust and Electron/protocol checks, then required CI evidence;
finish with `git diff --check`, status and self-review for scope, privacy and
user changes.

## Rollback и evidence

If evidence is incomplete, keep all plan files and active registry contracts;
do not claim prompt-strategy support from UI presence alone.
