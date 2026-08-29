# План 23.1 — TaskCheckpoint для compaction и recovery: Core-контракт, schema и storage

Статус: самостоятельный этап 1 для [плана 23.0](./23-0-task-checkpoint.md); issue: [https://github.com/rkfsociety/EvoHime/issues/7](https://github.com/rkfsociety/EvoHime/issues/7). Этап не означает, что функционал уже реализован.

## Цель

Зафиксировать и реализовать authoritative Core-контракт для направления «TaskCheckpoint для compaction и recovery», чтобы результат «versioned `TaskCheckpoint` durable contract и immutable parent chain» имел versioned schema, проверяемые ограничения, provenance и предсказуемую persistence policy.

## Граница этапа

- В этом файле проектируются Rust-типы, validator/policy matrix, canonical serialization, error codes и storage contract; runtime orchestration и UI выполняются в следующих этапах.
- Core — единственный владелец состояния и решений. Model/user input может предложить данные, но не подтверждает capability, approval, effect, test или завершение.
- Кандидатные поверхности из обзора: crates/evohime-core, crates/evohime-local-storage, crates/desktop-ipc, Electron и focused tests. Их точные имена и новые файлы подтверждаются до изменения на evidence freeze.

## Зависимости

### Блокирующие

- План 23.0 — утверждённые scope, requirements, non-goals и карта зависимостей.
- Действующие Core-owned capability/policy/approval, SQLite transaction/migration, event journal и authenticated IPC boundaries.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация по шагам

0. Сопоставить обзор с live checkout: текущий schema version, существующие типы/таблицы, свободные IPC tags и тесты. Если контракт уже есть, подготовить evidence для закрытия вместо второй реализации.
1. Выписать поля, enum/state transitions, actor/provenance, scope, idempotency, limits, sensitivity и compatibility rules. Для каждой mutable операции определить expected version и stale outcome.
2. Реализовать serde/JSON/Proto representation и canonical hash только из нормализованных полей; неизвестная версия, лишние поля с authority-эффектом и превышение лимита должны давать typed error.
3. Реализовать Core validation перед persistence: path/scope/capability/approval/policy checks не делегируются renderer или imported content.
4. Если состояние durable, добавить отдельный storage module и additive transactional migration через существующий migration ladder с backup-before-migrate. Если состояние ephemeral, явно закрепить это тестом и не добавлять фиктивную таблицу.
5. Добавить deterministic fixtures для valid/invalid schema, duplicate/idempotency, stale version, redaction, size limits и migration failure; зафиксировать формат evidence для этапа 2.

## Артефакты выхода

- versioned Rust contract и validator с documented state/transition table;
- canonical serialization/hash и стабильные typed error codes;
- storage schema/store либо доказательство отсутствия durable state;
- provenance/sensitivity matrix и negative security fixtures;
- краткий evidence record с exact paths, schema revision и командами проверки.

## Критерии выхода

- [ ] versioned `TaskCheckpoint` durable contract и immutable parent chain.
- [ ] Core-derived evidence отделён от model-proposed summary.
- [ ] checkpoint создаётся до compaction и используется после replay recovery.
- [ ] Contract не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Migration/rollback или отсутствие persistence доказаны focused tests.

## Rollback и отказ

При ошибке миграции восстановить backup и оставить старую schema revision читаемой. При несовместимой записи вернуть typed unsupported/invalid без частичной записи. Не выполнять внешние side effects на этом этапе.

## Не входит

Runtime scheduling/orchestration, IPC UI, внешний provider/backend, автоматическая активация и любые необратимые эффекты.
