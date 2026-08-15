# Этап 02.4: Chain storage и export

Этап плана [02 Подписанные hash-chain receipts](02-0-signed-hash-chain-receipts.md).

## Зависимости

Блокирующие: этапы 02.1 (canonical bytes для проверки), 02.2 (public key) и
02.3 (сами receipts).

Это последний этап плана: он показывает и проверяет результат остальных.

## Что этап отдаёт наружу

Verify-chain, read-only IPC `ListReceipts`, `VerifyReceipts`, `ExportReceipts`
и статус цепочки в UI.

## Содержание

- Хранить bounded receipt metadata в SQLite и append-only JSONL export.
- Данные arguments/results хранить отдельно только по существующим privacy и
  retention правилам.
- Добавить verify-chain: signature, canonical bytes, previous hash и approval
  binding.
- Отдельно диагностировать broken chain, stale key, digest mismatch и missing
  receipt.

## IPC и UI

- Read-only `ListReceipts`, `VerifyReceipts`, `ExportReceipts` с limit и date
  bounds.
- UI показывает status `verified`, `broken`, `unverified`, key id и hashes;
  не показывает секретный payload автоматически.
- Approval preview должен показывать action digest, чтобы пользователь видел,
  что именно он подтверждает.

## Проверки

- deletion/reordering test для hash chain;
- broken chain, stale key, digest mismatch и missing receipt диагностируются
  раздельно;
- UI не показывает секретный payload автоматически.

## Критерии готовности

- chain break виден пользователю и в diagnostics;
- текущий audit trail остаётся совместимым на переходный период.
