# План 24.1 — Agent Skills registry и progressive disclosure: Core-контракт, schema и storage

Статус: самостоятельный этап 1 для [плана 24.0](./24-0-agent-skills.md); issue: [ссылка](https://github.com/rkfsociety/EvoHime/issues/3). Этап не означает, что функционал уже реализован.

## Цель

Зафиксировать и реализовать authoritative Core-контракт направления «Agent Skills registry и progressive disclosure», чтобы результат «global/project/compatibility discovery работает с deterministic precedence» имел версионируемую schema, limits, provenance и предсказуемую persistence policy.

## Граница этапа

- Этот файл покрывает модель данных, validator/policy matrix, canonical serialization, error codes и storage contract. Runtime orchestration и client surface выполняются в последующих файлах.
- Core является единственным владельцем состояния и решений. Model/user input может быть proposal, но не доказывает capability, approval, effect, test или завершение.
- Кандидатные поверхности из обзора: crates/evohime-core, crates/evohime-local-storage, crates/desktop-ipc, Electron и focused tests. Точные пути, schema revision и свободные IPC tags подтверждаются на шаге 0.

## Зависимости

### Блокирующие

- План 24.0 — утверждённые scope, requirements, non-goals и карта зависимостей.
- Действующие Core-owned capability/policy/approval, SQLite transaction/migration, event journal и authenticated IPC boundaries.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация по шагам

0. Сопоставить обзор с live checkout: существующие типы/таблицы, schema version, тесты и свободные идентификаторы. Если контракт уже реализован, собрать evidence для закрытия вместо дублирования.
1. Описать поля, enum/state transitions, scope, actor/provenance, idempotency, limits, sensitivity и compatibility rules; для каждой mutation определить expected version и stale outcome.
2. Реализовать serde/JSON/Proto representation и canonical hash из нормализованных полей. Unknown version, authority-bearing unknown fields и oversized input дают typed error.
3. Реализовать Core validation до persistence: path/scope/capability/approval/policy checks не делегируются renderer, imported content или модели.
4. Для durable состояния добавить отдельный storage module и additive transactional migration через существующий migration ladder с backup-before-migrate. Для ephemeral/CI-контура доказать отсутствие незаявленной persistence.
5. Добавить fixtures для valid/invalid schema, duplicate/idempotency, stale version, redaction, limits и migration failure; записать evidence, необходимое этапу 2.

## Артефакты выхода

- versioned Rust contract, validators и transition table;
- canonical serialization/hash и стабильные typed errors;
- storage schema/store либо доказательство отсутствия durable state;
- provenance/sensitivity matrix и negative security fixtures;
- evidence record с exact paths, schema revision и командами проверки.

## Критерии выхода

- [ ] global/project/compatibility discovery работает с deterministic precedence.
- [ ] invalid frontmatter, collision, oversized file и path escape дают typed error.
- [ ] model получает metadata catalog, полный skill загружается только on-demand.
- [ ] Контракт не расширяет capabilities и не переносит authority за пределы Core.
- [ ] Migration/rollback или отсутствие persistence доказаны focused tests.

## Rollback и отказ

При ошибке миграции восстановить backup и оставить предыдущую schema revision читаемой. Несовместимая запись даёт typed unsupported/invalid без частичной записи. Внешние side effects на этом этапе не выполняются.

## Не входит

Runtime scheduling/orchestration, IPC UI, внешний provider/backend, автоматическая активация и необратимые эффекты.
