# EvoHime — release evidence и rollback matrix

Этот документ описывает evidence для поставки. Artifact bundle должен быть
redacted: допускаются commit, contract/schema versions, test IDs, hashes,
typed outcomes, bounded metrics и recovery state; credentials, raw provider
output, transcripts, absolute paths и PII запрещены.

## Текущий статус выпуска

Статус: `TECHNICAL_GATES_PASS / RELEASE_GREEN`.

Последнее evidence для направлений TaskCheckpoint и Agent Skills зафиксировано
на коммите
`90853fd7` (29 августа 2026 года): contract
`TaskCheckpoint v1`, Agent Skills v1, SQLite schema `v32`, additive `desktop-ipc-v1` команды
`GetTaskCheckpoint`/`ResolveTaskCheckpoint` (tags 137–138) и typed события
`TaskCheckpointProjection`/`TaskCheckpointActionResult` (oneof tags 15–16).
Проверены 553 Core unit tests, 35 desktop-ipc tests, 196 local-storage tests,
466 Electron tests (2 skipped), production bundle checks, strict clippy,
rustfmt, protocol/typecheck, backup/restore, automation boundary, redaction и
license gates. IPC action outcomes typed and idempotent; внешние effects не
запускаются.

Свежая проверка запускается `scripts/final-release-audit.tests.ps1` и включает
Rust Core/storage/IPC tests, rustfmt, automation boundary, backup/restore и
redaction gates, Electron protocol и typecheck. Полный локальный прогон 26
августа 2026 года также подтвердил строгий `cargo clippy`, Electron `npm test`
(457 passed, 2 skipped), production build и bundle checks. Compatibility,
native-package, installer и upgrade/rollback gates проходят в Windows CI.
Documentation gate проверяет все tracked text-файлы, относительные Markdown-ссылки
и запрет устаревших удалённых audit-документов.

Code signing не входит в текущий release scope; manifest/hash остаётся
документированным trust root. Optional browser/voice/vision adapters работают
через typed `backend_unavailable` и не расширяют базовый runtime.

## Rollback / disable

| Компонент | До изменения | При crash/ошибке | Disable/cleanup | Evidence |
| --- | --- | --- | --- | --- |
| SQLite schema / automation tables | backup с checksum и schema version | restore safety backup; повторить migration только после проверки | удалить только expired snapshots/archive по retention | `evohime-local-storage` backup tests |
| Automation archive | canonical run/events/snapshots JSON с SHA-256 и сроком retention | transaction restores only after checksum and identity validation | `sweep_expired_archives` удаляет только истёкшие archives | `automation_store` archive/restore test |
| Core/supervisor package | полный install backup и transaction journal | transaction worker откатывает staging и очищает journal | остановить компонент, сохранить redacted diagnostic | `electron-fault` и installer rollback smoke |
| User-triggered self-repair update | isolated checkout, bounded diff/tests, commit SHA, CI state и installer marker | health timeout или failed startup возвращает полный backup | repair остаётся failed/recoverable, без повторного push или restart | Electron repair tests, updater health-marker tests, authenticated Core E2E |
| Optional browser/voice/vision adapter | capability manifest/hash и typed availability | `backend_unavailable`, без Core state mutation | disable adapter, remove staging/runtime cache | `decision-register.md`, adapter contract tests |
| Automation simulation | ephemeral state, fake-provider fixture | discard ephemeral state; no production recovery | delete temp workspace after run | automation A05/A06 fixtures |

Rollback не обещает откат уже совершённых внешних side effects: такие effects
идут через existing receipts/reconciliation и требуют typed unknown outcome.

## Evidence format

Каждый bundle содержит `manifest.jsonl` с полями `evidence_version`, `commit`,
`test_id`, `contract_version`, `environment_class`, `expected_outcome`,
`actual_outcome`, `event_sequence`, `artifact_sha256` и `redaction_status`.
Время и абсолютные пути не являются частью replay hash. Retention: CI evidence
1 день, local diagnostic export 7 дней, automation archive 30 дней, а durable
audit хранится по его собственному Core policy.

## Privacy, egress и maintenance

- базовый package не содержит credentials, Python/Node runtime, model assets,
  public HTTP listener или cloud control plane;
- optional adapter manifest содержит только stable ID/version/hash/license и
  typed availability, никогда не ключи и не raw media;
- `scripts/release-evidence.tests.ps1` проверяет обязательные документы,
  запускает focused backup/restore и redaction tests и запрещённые markers;
- license/attribution inventory ведётся в [`licenses/README.md`](licenses/README.md)
  и обновляется в том же коммите, что и новый distributed artifact.
