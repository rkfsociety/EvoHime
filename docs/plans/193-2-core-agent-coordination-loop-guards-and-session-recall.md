# План 193.2 — runtime coordination, delegation и recovery

Статус: active implementation contract. Этап следует после 193.1.

## Scope и изменяемые контракты
1. Подключить guard в существующий Core agent execution lifecycle перед очередным шагом и после завершения tool/child result. Не создавать второй agent loop или dispatch path.
2. Собирать bounded rolling signature window из observed events. Для repeated-call/no-progress применять policy: запросить смену стратегии, предложить делегирование, приостановить run или остановить его. Автоматический повтор того же вызова запрещён.
3. Считать budgets на task, agent и parent execution; child assignment наследует ограниченный остаток, а не получает новый полный бюджет. Concurrent fan-out ограничивается существующим resource pressure/admission.
4. Реализовать coordinator поверх Agent Organization Registry, Team SOP, TeamSession, collaboration bus и child handoff. Выбор исполнителя — только среди зарегистрированных доступных profiles с явным capability intersection и policy preflight.
5. Ввести deterministic result aggregation: проверка assignment revision, artifact/evidence provenance, output bounds и conflict detection. Неполный или противоречивый результат остаётся partial/conflicted и не становится готовым ответом без review.
6. Встроить session-query как read-only context retrieval до планирования и при смене стратегии. Запрос scoped к текущему workspace/task и разрешённым memory views; агент получает citations/refs, summaries и trust metadata, а не необработанную историю.
7. Recovery сверяет active assignments, task generation, deadlines и child handoff state. Повторное присоединение допустимо только к тому же idempotency/source revision; неизвестный исход остаётся unknown.
8. Ошибки и остановки выдавать typed outcomes, не маскировать успешным completion и не выполнять внешние side effects из recovery.

## Зависимости
### Блокирующие
193.1; существующие Core agent loop, TaskCoordinator, PermissionEngine, EventJournal, Agent Organization/TeamSession/collaboration/handoff, Context Namespace/memory recall/conversation history и resource admission.
### Опциональные
План 180 для remote A2A delegation; план 184 для post-run trace evaluation.

## Recovery и rollback
On restart: generation-fence незавершённые runs, восстановить только подтверждённые assignment/handoff refs, не делать blind resubmit. Если coordinator policy unavailable — сохранить исходный одиночный execution path и approval checks. Откат выключает coordination policy и guard hook, но не удаляет durable records и не повторяет эффекты.

## Verification
- Guard наблюдает фактические вызовы, а не только план модели.
- Repeated calls с одинаковыми args блокируются после порога; legitimate reads с доказанным прогрессом не блокируются.
- Child budget не превышает parent остаток; параллельность bounded.
- Child agent не получает parent credentials, полный workspace или grants.
- Conflict/late/stale/duplicate child reports не применяются автоматически.
- Crash/restart в каждой фазе handoff не создаёт повторный effect.
- Session query не пересекает workspace, task или MemoryView scope.
- Typed unavailable/partial outcomes сохраняются и видны caller-у.

## Release evidence
Redacted execution/assignment ids, generation/revision, policy version, loop reason codes, budget counters, child outcome classes, conflict counts и focused recovery tests. Не сохранять raw prompts/tool args, secrets, полные transcripts или artifact bodies.

## Критерии выхода
- [ ] Один agent loop и один tool dispatch path.
- [ ] Loop guard включён на поддерживаемых agent execution paths.
- [ ] Delegation ограничена parent budget, grants и pinned TeamSession.
- [ ] Recovery не повторяет ambiguous effects.
- [ ] Query scope и provenance проверяются Core на каждом запросе.
