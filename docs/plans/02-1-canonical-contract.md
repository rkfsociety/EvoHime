# Этап 02.1: Canonical contract

Этап плана [02 Подписанные hash-chain receipts](02-0-signed-hash-chain-receipts.md).

## Зависимости

Блокирующие: нет. Context Budget Manager реализован: payload содержит `context_ledger_hash`, определённый
именно там.

Разблокирует: все остальные этапы плана 02.

## Что этап отдаёт наружу

Canonical JSON encoding payload и shared known-answer vectors для всех
реализаций verifier.

## Содержание

- Зафиксировать canonical JSON encoding и versioned field rules.
- Ограничить payload/receipt size и запретить свободные raw strings.
- Добавить shared known-answer vectors для Rust, Electron verifier и будущего
  offline CLI.

## Проверки

- Rust Ed25519 known-answer and cross-implementation vectors;
- tamper tests для каждого payload field;
- bounded size и secret-redaction tests.

## Критерии готовности

- одинаковый payload даёт одинаковые canonical bytes во всех реализациях;
- receipt не содержит свободных raw strings и не превышает лимит размера.
