# План 193.3 — IPC, agent context и UI projections

Статус: active implementation contract. Этап следует после 193.1–193.2.

## Scope и изменяемые контракты
1. Добавить additive authenticated IPC для bounded guard status, coordinator assignments, child outcomes/conflicts и session-query requests/results. Если подходящие команды существуют, расширить их совместимо.
2. Проекция содержит task/agent/assignment ids, status, revision, policy version, budget counters, typed reason, timestamps и opaque evidence refs. Не передавать raw tool args, transcript body, secrets, full prompt или hidden reasoning.
3. Доставить агенту query results как bounded cited context с provenance refs и authority/trust labels. Retrieved content остаётся untrusted data и не меняет system policy, tool catalog или permissions.
4. Добавить в существующий Operations/Task UI представление guard state, причины остановки/паузы, budget usage, child assignments, partial/conflict states и источников recalled context.
5. Управление пользователем ограничить безопасными действиями: открыть evidence, повторить query, отменить текущую задачу либо использовать существующий review flow. UI не может снять guard, увеличить budget или одобрить инструмент.
6. Сохранить progressive disclosure: компактный статус по умолчанию, подробные причины и provenance по запросу. Отсутствие capability отображать как unavailable, а не пустой успешный результат.

## Зависимости
### Блокирующие
193.1–193.2; proto schema, authenticated IPC, generated bindings и существующие task/agent/memory projections.
### Опциональные
План 194 может переиспользовать общий task status UI; план 184 может предоставлять trace links.

## Recovery и rollback
Старый shell игнорирует additive events/commands. Unknown command/version даёт typed unavailable. Renderer не кеширует authoritative state между поколениями Core; после reconnect получает bounded snapshot и deduplicates events по sequence/revision. Rollback удаляет новые UI sections и IPC calls.

## Verification
- Proto tags уникальны; Rust и TypeScript projections совпадают.
- IPC bounds ограничивают assignments, refs, excerpt bytes и frame size.
- Tests проверяют отсутствие raw prompts, raw tool args, secrets, transcript bodies и hidden reasoning.
- UI отображает stopped, paused, partial, conflicted, unavailable, stale и reconnect states.
- UI controls не расширяют grants и не обходят guard.
- npm run check:protocol и focused renderer tests проходят.

## Release evidence
IPC command/event tags, projection schema version, bounds, redaction test results, focused UI tests и contract compatibility result.

## Критерии выхода
- [ ] UI показывает наблюдаемое Core состояние и typed reasons.
- [ ] Ни одно действие UI не обходит permission, budget или approval.
- [ ] Старый клиент совместим с additive IPC.
- [ ] История остаётся redacted и provenance-backed.
