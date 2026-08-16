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
  отказа.
- OperationsPanel показывает активных children, leases и dead-letter.
- Trace сохраняет state transitions, not raw hidden chain-of-thought.

## Проверки

- smoke test: one full researcher→implementer→tester→reviewer run;
- trace содержит переходы состояний, но не raw hidden chain-of-thought;
- dead-letter и потерянные leases видны в панели.

## Критерии готовности

- UI и audit показывают фактическое состояние workflow;
- ни одна поверхность не раскрывает скрытые рассуждения ребёнка.
