# План 23.2 — TaskCheckpoint для compaction и recovery: runtime-интеграция и recovery

Статус: самостоятельный этап 2 для [плана 23.0](./23-0-task-checkpoint.md); начинается только после закрытия [плана 23.1](./23-1-task-checkpoint.md).

## Цель

Подключить контракт «TaskCheckpoint для compaction и recovery» к Core runtime одним вертикальным срезом: validated request -> policy/capability/approval -> bounded execution -> typed result/event -> durable recovery.

## Зависимости

### Блокирующие

- План 23.1 — contract, validators, error codes и storage policy.
- Существующие workflow/child/provider/tool/memory boundaries, cancellation, budgets, audit и unknown-outcome semantics.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация по шагам

0. Проверить артефакты этапа 1 и зафиксировать immutable contract/policy snapshot для active run.
1. Реализовать Core handler/state machine без вызовов из renderer: вход валидируется, authorization повторяется непосредственно перед effect, результат и event связываются correlation/idempotency identity.
2. Подключить только предусмотренные обзором registry/workflow/child/provider/tool surfaces; optional dependency должна давать typed unavailable/degraded, а не неявный success.
3. Определить cancellation, timeout, lease, retry, backpressure, partial failure и unknown outcome. После restart разрешён только replay/reconciliation, но не blind retry side effect.
4. Добавить fault-injection для crash до/после dispatch marker, stale lease/version, duplicate delivery, policy change и corrupted state.
5. Подготовить metadata-only projection contract для этапа 3 и redacted diagnostic evidence для этапа 4.

## Артефакты выхода

- Core state machine и handler с повторной policy/capability/approval проверкой;
- durable event/recovery transitions либо documented ephemeral lifecycle;
- bounded cancellation/retry/lease/partial-failure outcomes;
- integration and fault-injection fixtures;
- список стабильных команд/events, передаваемых на этап 3.

## Критерии выхода

- [ ] Основной сценарий достигает typed result только после Core validation.
- [ ] Duplicate/stale/limit/cancel/restart/unavailable cases имеют отдельные outcomes.
- [ ] Unknown external effect не повторяется автоматически.
- [ ] Active run закреплён к exact contract/policy snapshot.
- [ ] Fault-injection и recovery tests воспроизводимы.

## Не входит

Новый клиентский authority, прямой доступ UI к storage, смена существующей security policy и необъявленный внешний network/runtime.
