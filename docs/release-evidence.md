# EvoHime — release evidence и rollback matrix

Этот документ описывает evidence для поставки. Artifact bundle должен быть
redacted: допускаются commit, contract/schema versions, test IDs, hashes,
typed outcomes, bounded metrics и recovery state; credentials, raw provider
output, transcripts, absolute paths и PII запрещены.

## Rollback / disable

| Компонент | До изменения | При crash/ошибке | Disable/cleanup | Evidence |
| --- | --- | --- | --- | --- |
| SQLite schema / automation tables | backup с checksum и schema version | restore safety backup; повторить migration только после проверки | удалить только expired snapshots/archive по retention | `evohime-local-storage` backup tests |
| Automation archive | canonical run/events/snapshots JSON с SHA-256 и сроком retention | transaction restores only after checksum and identity validation | `sweep_expired_archives` удаляет только истёкшие archives | `automation_store` archive/restore test |
| Core/supervisor package | полный install backup и transaction journal | transaction worker откатывает staging и очищает journal | остановить компонент, сохранить redacted diagnostic | `electron-fault` и installer rollback smoke |
| Optional browser/voice/vision adapter | capability manifest/hash и typed availability | `backend_unavailable`, без Core state mutation | disable adapter, remove staging/runtime cache | `decision-register.md`, adapter contract tests |
| Automation simulation | ephemeral state, fake-provider fixture | discard ephemeral state; no production recovery | delete temp workspace after run | automation A05/A06 fixtures |
| Windows signing | certificate-backed SHA-256 Authenticode + RFC3161 timestamp | fail-closed before publish | do not publish unsigned installer | `scripts/sign-windows-release.ps1`, Windows CI evidence |

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
- `scripts/signing-release-gate.tests.ps1` проверяет определение signing
  pipeline; реальный release требует secrets `EVOHIME_SIGNING_CERTIFICATE_BASE64`
  и `EVOHIME_SIGNING_CERTIFICATE_PASSWORD`, а также `signtool.exe` в Windows CI.
