# Этап 03.4: UI и observability

Этап плана [03 Специализированные child workflows](03-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: этапы 03.1 (что показывать), 03.2 (состояния и leases) и 03.3
(что можно показывать без утечки контекста).

Это последний этап последнего плана цепочки.

## Что этап отдаёт наружу

Timeline ролей, панель активных детей и trace переходов состояний.

## Что уже есть в коде

Есть: OperationsPanel считает события `child.*` и показывает число принятых
отчётов и падений.

Нет: разбивки по ролям, budget, evidence и причине отказа; активных leases и
dead-letter; trace переходов состояний.

## Содержание

- Task timeline показывает role, status, budget, evidence, approval и reason
  отказа из Core-owned projection; она не читает workspace, SQLite или raw
  report напрямую.
- OperationsPanel показывает активных children, leases и dead-letter.
- Dead-letter содержит окончательно не принятые reports/events после
  исчерпания bounded transport/recovery retries или revisions либо после
  невосстановимого schema/policy failure; лимит retries задаётся 03.2 и не
  является новым бесконечным циклом. Запись хранится 30 дней, доступна
  coordinator и администратору, payload redacted и не содержит raw
  transcript.
- Trace сохраняет state transitions, revision, lease outcome и correlation
  ids, not raw hidden chain-of-thought.

Минимальное событие projection содержит `event_id`, `parent_task_id`,
`child_task_id`, `role`, `revision`, `state`, `reason_code`, безопасные budget/
lease counters, `parent_sequence` и timestamp. `summary`, evidence и
acceptance criteria передаются только в bounded/redacted projection; raw
transcript, output payload и секреты запрещены. Dead-letter в UI — это только
запись с `dead_letter=true` из checkpoint, а не отдельный renderer-owned
журнал.

Audit и trace различаются: audit отвечает на «кто, когда, что запросил и чем
закончилось» и содержит actor, parent/child IDs, grants summary, approval,
result и reason; trace отвечает за порядок state transitions, lease/revision и
correlation IDs. Ни один из них не содержит raw chain-of-thought.

## Проверки

- smoke test: one full researcher→implementer→tester→reviewer run;
- trace содержит переходы состояний, но не raw hidden chain-of-thought;
- dead-letter и потерянные leases видны в панели.
- provenance, acceptance criteria, revision и partial-success reason видны в
  безопасной summary-проекции;
- audit и trace различимы и не раскрывают raw transcript.

## Критерии готовности

- UI и audit показывают фактическое состояние workflow;
- ни одна поверхность не раскрывает скрытые рассуждения ребёнка.
