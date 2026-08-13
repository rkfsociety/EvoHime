# Подплан 5 — schedules, Pulse и внешние каналы

Статус: самый сложный и последний подплан
Порядок: 5 из 5
Источник: бывший единый мастер-план; актуальная детализация находится в этом подплане.

## Цель

Подключить bounded schedule/trigger/monitor contract к supervisor и добавить безопасные proactive workflows без скрытой внешней мутации.

## Объём

- supervisor runtime wiring с теми же budgets, permissions, approvals и cancellation, что у обычного run;
- retry/backoff/dead-letter/requeue с причиной и audit;
- локальные источники: workspace changes, local files, task deadlines, CI status;
- GitHub notifications только через безопасную авторизацию и read-only scope;
- Pulse digest, новые события, missed runs и degradation;
- OAuth/browser authorization без токенов в prompt/trace/logs;
- ACP/external-agent gateway только после стабилизации локального контура.

## Порядок реализации

1. Подключить timer/event monitor к supervisor lifecycle и lease ownership.
2. Реализовать durable dead-letter/requeue UI и audit decisions.
3. Подключить локальные источники и bounded deduplication.
4. Реализовать Pulse digest и truthful failure/degradation notifications.
5. Добавить OAuth/browser authorization с отдельным secret boundary.
6. После security review подключать внешние каналы и ACP gateway.

## Критерии готовности

- missed/duplicate trigger, restart, cancellation и permission denial детерминированы;
- monitor не обходит run policy, approval или network allowlist;
- dead-letter сохраняет attempt count, backoff, причину и ручной requeue;
- Pulse не маскирует failed/degraded state успешным уведомлением;
- токены не попадают в traces, audit, exports или model context;
- внешняя мутация невозможна без отдельного approval.

## Зависимости

Требует подплан 4, supervisor recovery/leases и Core Doctor. Это последний этап: внешние каналы не должны блокировать локальный task runner.
