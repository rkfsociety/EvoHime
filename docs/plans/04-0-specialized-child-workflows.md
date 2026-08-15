# План 04: Специализированные child workflows

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

## Цель

Превратить существующий child runtime из общего handoff-механизма в несколько
ограниченных, проверяемых workflow: исследование, реализация, проверка и
review. Родительская Ева сохраняет ownership задачи, approval и финального
решения.

## Роли

- `researcher` — read-only workspace/RAG, возвращает evidence и unknowns;
- `implementer` — изменяет файлы только в выданном scope и через Core tools;
- `tester` — запускает разрешённые проверки, не меняет исходники;
- `reviewer` — проверяет diff, tests, policy и citations;
- `coordinator` — единственный, кто объединяет отчёты и решает следующий шаг.

Новые роли не получают скрытых полномочий: permission, sandbox и approval
проверяются Core повторно на каждом tool call.

## Контракт child task

Каждый child получает bounded:

- `parent_task_id`, `child_task_id`, role и goal;
- workspace/path grants;
- input context ids и expected output schema;
- token/time/tool-call budget;
- cancellation/deadline и acceptance criteria.

Отчёт содержит `status`, `summary`, `evidence[]`, `changed_paths[]`, `tests[]`,
`risks[]`, `next_action` и `provenance`. Raw transcript не передаётся родителю
по умолчанию.

## Workflow patterns

### Исследование → реализация → тест → review

- researcher формирует bounded evidence;
- coordinator строит plan snapshot;
- implementer работает только по approved scope;
- tester проверяет фактический результат;
- reviewer может вернуть `revise` с конкретными evidence.

### Параллельное исследование

Несколько read-only researcher children получают разные области. Coordinator
делает fan-in, удаляет дубликаты и разрешает конфликт источников до передачи
implementer.

### Условный recovery

При tool failure запускается только соответствующий recovery child. Три
повторяющихся failure patterns переводят задачу в `revise_plan`, а не создают
новых бесконтрольных детей.

## Что уже есть в коде

`crates/evohime-core/src/child_roles.rs` и `child_runtime.rs` уже содержат
существенную часть: перечисление ролей (`Coordinator`, `Researcher`,
`Implementer`, `Reviewer`, `Tester`), lifecycle state machine ровно в тех
состояниях, что описаны ниже, typed `ChildTaskRequest`/`ChildReport` с
валидацией до persistence, запрет вложенных детей и разрешение только
read-only capabilities.

Чего нет: workspace/path grants, token/time/tool-call budget, correlation id
на receipt, offload в artifact store и изоляция контекста между детьми. Роль
`researcher` существует как имя, но без RAG у неё нет своего инструмента.

## Этапы

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 04.1 | [Typed contracts](04-1-typed-child-contracts.md) | typed input/output child task и correlation ids | 04.2–04.4 |
| 04.2 | [Coordinator state machine](04-2-coordinator-state-machine.md) | состояния, leases и restart recovery | 04.3, 04.4 |
| 04.3 | [Context isolation](04-3-child-context-isolation.md) | изоляция контекста и offload | 04.4 |
| 04.4 | [UI и observability](04-4-child-ui-and-observability.md) | timeline, OperationsPanel и trace | UI |

## Зависимости плана

Блокирующие:

- Context Budget Manager (реализован, см. [`../architecture.md`](../architecture.md)) — budget ребёнка, context isolation и offload больших
  результатов в artifact store;
- этапы 01.2 и 01.3 — роль `researcher` определена как read-only доступ к
  workspace/RAG и без retrieval с planner не имеет своего инструмента;
- этап 02.3 — связь действий ребёнка с approval родителя;
- существующие child runtime, permission policy, task graph, leases и
  evaluation catalog (`tests/evals/`).

Это последний план цепочки: от него никто не зависит. A2A/network protocol
не нужен — достаточно локальных Core-owned children.

## Критерии готовности плана

- каждый child имеет typed input/output и отдельный budget;
- parent никогда не принимает child result без validation;
- child не расширяет права родителя и не обходит approval;
- restart/cancellation не оставляют orphan processes or leases;
- UI и audit показывают фактическое состояние workflow.
