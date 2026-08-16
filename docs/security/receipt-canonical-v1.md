# Receipt canonical contract v1

Это нормативный контракт receipts этапа 01.1. Источник машинных ограничений —
`contracts/receipts/v1/limits.json`, схема — `receipt.schema.json`, а
cross-language known-answer vectors — `vectors.json`.

## Bytes и hash

Payload канонизируется по RFC 8785 JCS: UTF-8 без BOM и завершающего LF,
ключи сортируются по UTF-16 code units, Unicode не нормализуется. Payload
подписывается ровно этими bytes. Envelope имеет поля `payload`, `key_id`,
`signature_algorithm`, `signature`; его `receipt_hash` равен
`lowercase_hex(SHA-256(canonicalize(envelope)))` после добавления подписи.

`result_hash` вычисляется как SHA-256 от `evohime-result-v1\0` и JCS bounded
result projection. Raw arguments, prompts, responses, paths, stdout/stderr и
секреты в receipt не входят.

## Границы и отказ

Входной envelope ограничен 8192 bytes, payload — 4096 bytes, identifier —
128 ASCII bytes, JSON depth — 4. До schema/canonical verification вход
проходит проверки UTF-8, JSON и duplicate keys. Стабильные коды отказа
опубликованы в `version-manifest.json`; verifier не исправляет malformed или
non-canonical input.

V1 dispatch выполняется по `receipt_version`. Неизвестная версия даёт
`receipt.unsupported_version`, а не `receipt.signature_invalid`; изменение
полей, типов, enum, conditional rules или подписываемых bytes требует v2.

## Реализации

Rust consumer находится в `crates/evohime-receipts`, Electron consumer — в
`desktop/evohime-electron/src/shared/receipt-contract.ts`. Обе реализации
проверяются одним `vectors.json` командой
`scripts/check-receipt-vectors.ps1`.
