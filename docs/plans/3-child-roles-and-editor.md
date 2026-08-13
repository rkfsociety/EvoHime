# Подплан 3 — child roles и native workflow editor

Статус: средняя сложность
Порядок: 3 из 5
Источник: [evohime-master-plan.md](evohime-master-plan.md)

## Цель

Подключить существующие bounded child-role/handoff contracts к реальному read-only выполнению и сделать его наблюдаемым в WinUI.

## Объём

- дочерние задачи onboarding, code search, threat-model review, test-plan review и documentation;
- выдача урезанного context и уникального `child_task_id`;
- отдельные filesystem/network sandbox и запрет write, shell, commit, install, network mutation и nested child;
- bounded `max_output_bytes`, timeout, cancellation и report validation;
- проверка родителем report, confidence и sources перед включением в plan/build;
- WinUI catalog, workflow editor, timeline, evidence и blocked/error states.

## Порядок реализации

1. Ввести Core dispatcher для разрешённых `ChildTaskKind` и хранение lifecycle.
2. Подключить read-only execution adapters с единым budget/timeout/cancellation.
3. Реализовать parent acceptance gate и evidence provenance.
4. Добавить IPC events/replay и reconnect для child timeline.
5. Добавить native catalog/editor и visual smoke.

## Критерии готовности

- child не может получить elevated permissions, создать child или выполнить mutation tool;
- timeout/cancel/oversized output приводят к bounded terminal state;
- parent принимает только валидный report с confidence/sources;
- UI одинаково показывает running, waiting approval, blocked, failed и completed;
- replay после reconnect не дублирует child events.

## Зависимости

Требует task lifecycle/checkpoints и завершённого child runtime policy. Для удобного запуска рекомендуется подплан 4, но security boundary должен быть самостоятельным.
