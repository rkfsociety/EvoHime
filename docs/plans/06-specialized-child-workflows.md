# План: Специализированные child workflows

Статус: draft для ревью.

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

## Этапы

### 06.1 Typed contracts

- Расширить existing child IPC/storage additive-полями role, grants, budget,
  input/output schema и parent sequence.
- Валидировать report schema до persistence и fan-in.
- Добавить correlation ids для task, child, tool call и receipt.

### 06.2 Coordinator state machine

- Зафиксировать Created → Queued → Running → Validating →
  WaitingParentAcceptance → Accepted/Rejected/Failed/Cancelled.
- Не считать child success финальным task success.
- Дочерние leases, cancellation и restart recovery должны быть bounded.
- После restart coordinator восстанавливает только durable checkpoint и
  повторно валидирует report/evidence.

### 06.3 Context isolation

- Child получает только selected context и свой scratchpad.
- Большие результаты offload в artifact store; parent получает summary + ids.
- Не передавать секреты соседнему child или role без policy grant.
- Reviewer видит diff/evidence, но не получает право менять код.

### 06.4 UI и observability

- Task timeline показывает role, status, budget, evidence, approval и reason
  отказа.
- OperationsPanel показывает активных children, leases и dead-letter.
- Trace сохраняет state transitions, not raw hidden chain-of-thought.

## Проверки

- role permission matrix и negative tests;
- malformed report, oversized report и wrong parent id;
- sequential, concurrent, conditional workflow fixtures;
- cancellation/restart/lease-loss recovery;
- reviewer rejection → bounded revision;
- child cannot commit/push without parent policy and approval;
- fan-in deterministic ordering and conflict reporting;
- smoke test: one full researcher→implementer→tester→reviewer run.

## Критерии готовности

- каждый child имеет typed input/output и отдельный budget;
- parent никогда не принимает child result без validation;
- child не расширяет права родителя и не обходит approval;
- restart/cancellation не оставляют orphan processes or leases;
- UI и audit показывают фактическое состояние workflow.

## Зависимости

Блокирующие:

- этапы 01.1 и 01.2 — budget ребёнка, context isolation и offload больших
  результатов в artifact store;
- этапы 03.2 и 03.3 — роль `researcher` определена как read-only доступ к
  workspace/RAG и без retrieval с planner не имеет своего инструмента;
- этап 04.3 — связь действий ребёнка с approval родителя;
- существующие child runtime, permission policy, task graph, leases и
  evaluation catalog (`tests/evals/`).

Этап 06.1 (typed contracts) зависит только из этого списка от 04.3, поэтому
его можно начать раньше остальных этапов плана.

Это последний план цепочки: от него никто не зависит. A2A/network protocol
не нужен — достаточно локальных Core-owned children.
