# План 05.7 — Crash recovery

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — честное закрытие committed запросов без terminal outcome после аварийного завершения: ни удаления, ни выдуманного успеха.

## Зависимости

### Блокирующие

- [05.2](05-2-durable-storage.md) — статус и `completed_at` в `model_requests`;
- [05.3](05-3-request-integration.md) — terminal outcome как часть pipeline;
- существующие recovery foundation и supervisor.

### Опциональные

- [05.5](05-5-receipt-and-tool-linkage.md) — signed request receipt. Без неё recovery закрывает статус в базе, но не оставляет подписанного следа о самом факте reconcile; появление 05.5 добавляет receipt на переход в terminal outcome.

## Правила

Committed request без terminal outcome после restart не удаляется.

Recovery должен присвоить честный explicit outcome:

```text
interrupted
```

если отсутствие завершения доказуемо, либо:

```text
unknown_outcome
```

если реальное внешнее состояние нельзя доказать.

Нельзя автоматически превращать неизвестный request в success или ordinary failure.

Общий принцип:

```text
never erase incomplete work; close or reconcile it explicitly
```

Partial stream после cancellation/crash нельзя тихо считать normal complete response — это то же требование, что в [05.5](05-5-receipt-and-tool-linkage.md), и оно проверяется с обеих сторон.

## Тесты

### Unit

- переход в `interrupted` только при доказуемом отсутствии завершения;
- переход в `unknown_outcome` во всех остальных случаях;
- запрет перехода в `success` из recovery.

### Integration

1. **Crash:** envelope committed до response; recovery сохраняет request и честный `interrupted`/`unknown_outcome`.
2. **Повторный restart:** уже закрытый recovery-статус не переписывается.

## Критерии готовности

1. После аварийного завершения ни один committed envelope не исчезает.
2. Ни один незавершённый request не получает статус успеха автоматически.
