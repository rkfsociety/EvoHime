# Подплан 4 — bounded task runner и model routing

Статус: центральный runtime-подплан
Порядок: 4 из 6; UI-части выполняются после подплана 0
Источник: бывший единый мастер-план; актуальная детализация находится в этом подплане.

## Цель

Соединить готовые task graph, context, research, capability selection, routing, run-policy и recovery в один bounded end-to-end запуск.

## Объём

- выбрать `next_ready`;
- собрать task, acceptance criteria, non-goals, research, skill, role, policy и route context;
- выполнить bounded run с checkpoint/effect/reconciliation;
- применить `run_policy`: max iterations, wall-clock, tool/token budget, network policy, approval mode и stop conditions;
- автоматически остановиться на approval, failure, unexpected diff, budget, scope drift или неясном acceptance criterion;
- подключить routing к provider gateway и UI;
- записывать redacted provider/model, latency, tokens, retries, estimated cost и decision reason;
- предложить следующий `next_ready` шаг после terminal outcome.

## Порядок реализации

1. Создать Core-owned `RunPolicy` snapshot и durable run start/stop events.
2. Реализовать runner orchestration без cloud fallback по умолчанию.
3. Подключить tool budgets, token/cost accounting, approval and cancellation gates.
4. Подключить provider routing runtime к gateway и visible fallback в UI.
5. Реализовать pause/resume/stop/recovery и следующий шаг.
6. Провести offline, provider-unavailable, scope-drift и Job Object integration tests.

## Критерии готовности

- один запуск проходит от `next_ready` до bounded terminal state через Core;
- каждый effect имеет checkpoint, idempotency key и recovery decision;
- превышение любого бюджета останавливает run с понятной причиной;
- route/fallback видны пользователю и не скрывают cloud execution;
- approval preview immutable и соответствует фактически исполняемому вызову;
- после перезапуска нет blind retry и не возникает второго runner.

## Зависимости

Требует этапы 0b/0c, 1–4, routing runtime contract и Core Doctor. Это основной интеграционный блок, поэтому его end-to-end реализация сложнее отдельных контрактов.
