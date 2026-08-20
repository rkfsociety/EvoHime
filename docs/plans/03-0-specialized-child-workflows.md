# План 03: Специализированные child workflows

Обзор плана. Этапы вынесены в отдельные файлы и ревьюятся по одному.

## Цель

Превратить существующий child runtime из общего handoff-механизма в несколько
ограниченных, проверяемых workflow: исследование, реализация, проверка и
review. Родительский coordinator (в пользовательском UI — Ева) сохраняет
ownership задачи, approval и финального решения. «Ева» здесь не отдельный child
с собственным lifecycle, а имя родительского процесса/агента.

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

`acceptance criteria` формулирует coordinator при создании child в виде
проверяемых условий. В контракте также задаётся `max_revisions` (по умолчанию
2, абсолютный максимум 3); новая итерация получает новый `revision` и не может
расширить исходные grants или budget.

Отчёт содержит `status`, `summary`, `evidence[]`, `changed_paths[]`, `tests[]`,
`risks[]`, `next_action` и `provenance`. `provenance` обязан включать хеши
входных данных и evidence, версии схемы и инструментов, model/provider IDs,
`created_at`/`completed_at` и `parent_sequence`; Core проверяет эти значения
до принятия отчёта. Raw transcript не передаётся родителю по умолчанию и
может быть выдан только по явному policy grant для диагностики.

Workspace/path grant — это подмножество grant родителя: canonical path
нормализуется и проверяется на containment без prefix-обхода, capability и
режим доступа должны присутствовать у родителя, а абсолютные пути вне
workspace и расширение scope отклоняются. Эта проверка и применение
выполняются Core на каждом tool call. Implementer получает только разрешённые
Core tools; прямой доступ к workspace/SQLite и обход Core запрещены.

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
implementer. Fan-in детерминирован: сначала более свежий published source с
валидной provenance, затем более специфичный path/chunk scope, затем меньший
`parent_sequence`, затем лексикографический `content_hash`. Неразрешённые
конфликты попадают в `unknowns` и блокируют implementer до решения coordinator.

### Условный recovery

При tool failure запускается только соответствующий recovery child. Три
повторяющихся failure patterns переводят задачу в `revise_plan`, а не создают
новых бесконтрольных детей. После исчерпания `max_revisions` coordinator
переводит задачу в `revise_plan`, если изменились предпосылки/границы, иначе в
`Failed`; автоматически создавать ещё один implementer нельзя.

Если провалено обязательное acceptance criterion, весь child получает
`revise`. Если провалены только необязательные проверки, результат может быть
`Accepted` только с перечисленными рисками и approval coordinator. Частичный
rollback не является автоматическим и допускается лишь отдельной approved
policy-операцией.

## Что уже есть в коде

`crates/evohime-core/src/child_roles.rs` и `child_runtime.rs` уже содержат
существенную часть: перечисление ролей (`Coordinator`, `Researcher`,
`Implementer`, `Reviewer`, `Tester`), lifecycle state machine ровно в тех
состояниях, что описаны ниже, typed `ChildTaskRequest`/`ChildReport` с
валидацией до persistence, запрет вложенных детей и базовое разрешение только
read-only capabilities. 03.1 добавляет typed grants для implementer/tester,
но не снимает повторную Core-проверку и approval.

Есть: Context Budget Manager и content-addressed Artifact Store в Core/SQLite
(schema v19), включая hash-проверку, task namespace и locator access для
владельца и его детей. Чего нет именно для child workflow: child-specific
token/time/tool-call budget, workspace/path grants с enforcement на каждом
tool call, correlation id на receipt, checkpoint coordinator и изоляция
контекста между детьми. Роль `researcher` существует как имя: workspace
retrieval для неё уже есть, но grants и budget не заданы.

## Этапы

| Этап | Файл | Что отдаёт наружу | Кто потребляет |
| --- | --- | --- | --- |
| 03.1 | [Typed contracts](03-1-typed-child-contracts.md) | typed input/output child task и correlation ids | 03.2–03.4 |
| 03.2 | [Coordinator state machine](03-2-coordinator-state-machine.md) | состояния, leases и restart recovery | 03.3, 03.4 |
| 03.3 | [Context isolation](03-3-child-context-isolation.md) | изоляция контекста и offload | 03.4 |
| 03.4 | [UI и observability](03-4-child-ui-and-observability.md) | timeline, OperationsPanel и trace | UI |

## Зависимости плана

Блокирующие:

- Context Budget Manager и Artifact Store (реализованы, см.
  [`../architecture.md`](../architecture.md)) — базовый ledger/offload и
  storage; child-specific budget, policy grants и access checks реализуются в
  03.1–03.3;
- Local Agentic RAG (реализован, см. [`../architecture.md`](../architecture.md)) — workspace retrieval и query planner, на
  которые опирается read-only роль `researcher`;
- существующие child runtime, permission policy, task graph и evaluation
  catalog (`tests/evals/`). Отдельного lease-механизма в текущем runtime нет;
  bounded leases и их checkpoint появляются в 03.2.

Опциональные:

- этап 01.3 — связь child-действий с receipt и approval родителя. До его
  появления `receipt_id` остаётся `None`: workflow работает, но audit явно
  показывает отсутствие receipt-связи; это не блокирует этапы 03.1–03.4.

Это последний план цепочки: от него никто не зависит. A2A/network protocol
не нужен — достаточно локальных Core-owned children.

## Критерии готовности плана

- каждый child имеет typed input/output и отдельный budget;
- parent никогда не принимает child result без validation;
- child не расширяет права родителя и не обходит approval;
- restart/cancellation не оставляют orphan processes or leases;
- fan-in конфликтов детерминирован и выполняется до implementer;
- grants не расширяются дочерним task и применяются на каждом tool call;
- bounded revision предотвращает бесконечный цикл и задаёт переход в
  `revise_plan`/`Failed`;
- checkpoint, provenance, partial success, dead-letter и различие audit/trace
  определены в дочерних этапах;
- UI и audit показывают фактическое состояние workflow.
