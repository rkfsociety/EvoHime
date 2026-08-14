# Этап 03.3: Runtime integration

Этап плана [03 Подписанные hash-chain receipts](03-0-signed-hash-chain-receipts.md).

## Зависимости

Блокирующие: этапы 03.1 (payload) и 03.2 (ключ); существующие approval и
exact-call hash.

Разблокирует: 05.1 — child workflows связывают действия ребёнка с approval
родителя именно через этот механизм.

## Что этап отдаёт наружу

Pre/post-action receipts и binding действия к human approval digest.

## Содержание

- Создавать pre-action receipt после policy/approval decision, но до mutation.
- Создавать post-action receipt с result hash и status после выполнения.
- Связывать action с human approval digest; изменение args между approval и
  execution должно блокироваться.
- Для read-only действий применять configurable sampling, для mutations —
  полный audit.

## Проверки

- approval digest substitution, stale approval и expired approval блокируются;
- изменение args между approval и execution не проходит;
- recovery test: crash между pre и post receipt оставляет verifiable pending
  state, а не поддельный success.

## Критерии готовности

- любое mutation action имеет receipt или явный refusal с причиной;
- receipt не утверждает correctness или policy enforcement сверх фактически
  проверенных полей.
