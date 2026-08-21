# 12-2 — Deterministic evaluation harness

## Цель

Запускать recorded scenarios и replay без production side effects.

## Изменения

1. Ввести fixture format с Core/schema/provider/model versions, recorded inputs,
   expected action trace, final-state predicates и evidence links.
2. Реализовать replay model/tool events с controlled provider responses,
   deterministic clock/limits и bounded artifact output.
3. Сравнивать expected action trace, terminal state, policy decisions,
   receipts, citations и unknown/degraded outcomes.
4. Поддержать repeated trials и reliability metrics без изменения production
   SQLite или workspace.
5. Пометить неполный/повреждённый trace typed diagnostic error, а
   невоспроизводимый output — `unknown`.

## Проверки

- одинаковый fixture даёт одинаковый result;
- state predicates и expected trace mismatch;
- replay после restart/reconnect;
- timeout, retry, cancellation и partial failure;
- отсутствие production filesystem/network side effects.

## Готово, когда

Evaluation воспроизводится offline из recorded inputs и не может выполнить
реальное опасное действие.
