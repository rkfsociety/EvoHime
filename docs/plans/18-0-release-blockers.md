# План 18 — закрытие release blockers

Этот обзорный план закрывает решения O-AUTO-01, O-AUTO-02, O-LIC-01 и
O-SIGN-01 из [`../decision-register.md`](../decision-register.md). Реализация
идёт строго последовательно: scheduler/IPC → archive/restore → license
inventory → code signing.

## Источник истины и текущая граница

Планы 01–17 завершены. В текущем checkout automation contract/runtime,
simulation и acceptance fixtures уже существуют, но scheduler и additive
automation IPC не подключены. Backup/restore для общей SQLite уже реализованы,
но отдельная automation archive/restore transaction и retention sweep
отсутствуют. License inventory и signing pipeline пока не дают release evidence.

## Зависимости

### Блокирующие

- планы 08–12 и существующие workflow/automation contracts;
- authenticated desktop IPC и Core-owned SQLite;
- Windows CI/package workflow для финального evidence.

### Опциональные

- browser/voice/vision adapters: при отсутствии остаются typed
  `backend_unavailable` и не блокируют базовую automation;
- внешний signing service: до его подключения локальная разработка продолжает
  использовать manifest/hash trust root, но release остаётся заблокированным.

## Этапы

1. **18.1 Scheduler и automation IPC**
   - Core-owned scheduler с timezone policy, deterministic clock, missed-tick
     classification, duplicate/idempotency fencing и durable events;
   - additive protobuf-команды/события для create/update/enable/disable,
     manual trigger, run status, history и cancellation;
   - Electron projection без scheduler/execution authority;
   - deterministic tests, migration/rollback notes, protocol/typecheck и
     clean-package acceptance.

2. **18.2 Automation archive/restore**
   - transactional archive/restore для automation runs, snapshots, events и
     dead-letter records;
   - safety backup, checksum, schema/version validation, bounded restore и
     retention sweep с redacted evidence;
   - crash/recovery/idempotency tests и release evidence.

3. **18.3 License inventory**
   - полный metadata inventory каждого distributed artifact, версии/коммиты,
     license text, source и SHA-256;
   - проверка inventory против Cargo/npm/installer/listener manifests;
   - CI gate, запрещающий installer audit при неполном или несовпадающем
     inventory.

4. **18.4 Code signing**
   - reproducible Windows signing step для installer и shipped binaries;
   - certificate identity, timestamp, signature verification и redacted CI
     evidence;
   - fail-closed release gate без credentials в репозитории и логах.

## Общие критерии завершения

- каждый этап имеет focused tests и подключённый CI gate;
- Core остаётся единственным владельцем durable state и эффектов;
- simulation не получает host/network/process authority;
- docs/current-state, decision-register, release-audit и release-evidence
  обновляются в том же task-only коммите;
- `cargo fmt`, protocol/typecheck, relevant tests и `git diff --check`
  проходят;
- после 18.4 `TECHNICAL_GATES_PASS / RELEASE_BLOCKED` меняется на
  `TECHNICAL_GATES_PASS / RELEASE_GREEN` только при наличии реального evidence.
