# Plan executor with bounded replan

> Дата: 2026-07-16  
> Статус: approved (approach A)

## Цель

Закрыть `6.11`: явный цикл `plan → execute → observe → replan → respond` в `agent-runtime`, с исполнением независимых шагов батчами через `task-engine::dependency_batches`.

## Поведение

1. **Initial plan** — как сейчас: planning LLM → `parse_plan` → `agent.plan.updated`
2. **Execute** — `dependency_batches(plan)`; внутри батча шаги выполняются параллельно (`tokio::join` / `JoinSet`); зависимые батчи строго по порядку
3. **Observe** — собрать текстовый summary успешных/проваленных шагов
4. **Replan** — до `MAX_REPLAN_ROUNDS` (3) раз вызвать planning-route с observe-контекстом; ответ:
   - `{ "done": true }` → выход к respond
   - `{ "done": false, "steps": [PlanStep...] }` → дописать шаги, emit `agent.plan.updated`, execute только новые
5. **Respond** — финальный streaming ответ как сейчас; опциональный tool.call из ответа сохраняем

## Ошибки

- Цикл/unknown dependency в батчах → `PlanStepFailed`
- Mutating tool fail → abort как сейчас
- Read/list/search fail → мягко, шаг failed, можно продолжать
- Replan parse fail → считать `done: true` и идти в respond (не крутить вечно)

## Вне скоупа

- `6.12` richer checkpoints
- UI Tasks/Actions deeper
- Correlation-id observability

## Тесты

- unit: replan JSON parse (`done` / steps)
- unit/integration: batches execute independent steps (mock tools)
- existing agent_loop tests остаются зелёными
