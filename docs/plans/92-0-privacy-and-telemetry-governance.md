# План 92.0 — Privacy & Telemetry Governance: consent, typed analytics events и sensitive-data boundaries

Статус: предложено по [issue #72](https://github.com/rkfsociety/EvoHime/issues/72). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime формальный **Privacy & Telemetry Governance** слой: единый Core/desktop contract для продуктовой аналитики и operational telemetry с явным consent lifecycle, строго типизированным event catalog, allowlisted properties, deterministic dedup и запретом на отправку содержимого conversations/workspace/secrets по умолчанию.

Главный принцип:

> Наблюдаемость продукта не должна становиться скрытым вторым каналом экспорта пользовательских данных.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/privacy-and-telemetry-governance.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 40.0 — Sensitive Data Guardrails: PII/secret detection и streaming redaction на model/tool boundaries.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 37.0 — Agent Middleware Pipeline: typed hooks вокруг model/tool execution.
- План 51.0 — Causal Collaboration Bus: typed pub/sub для team agents поверх child mailbox.
- План 69.0 — Runtime Intervention Pipeline: Core-owned middleware for agent messages and tool boundaries.
- План 81.0 — Event Visualizer Registry: typed renderers для tool, workflow и artifact events.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

## План реализации

1. Зафиксировать versioned typed contract, state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core validation и durable storage/event transitions. Миграция
   должна быть additive, транзакционной, с backup/recovery и deterministic
   serialization/hash там, где сущность versioned.
3. Подключить существующие registry/tool/workflow/provider/child контуры,
   повторные grant/policy/approval проверки и bounded retry/cancellation.
4. Добавить additive IPC, main/preload adapter и metadata-only renderer/UI;
   sensitive payload, raw prompt/output и credentials не передавать.
5. Провести focused unit/storage/integration/recovery/security/eval tests,
   обновить architecture/current-state только после фактической реализации
   и сохранить команду воспроизведения проверки.

## Критерии готовности из issue

- [ ] Есть explicit per-category consent state.
- [ ] Есть централизованный typed telemetry dictionary/API.
- [ ] Properties allowlisted, unknown fields fail closed.
- [ ] Sensitive content deny-by-default и проходит pre-send guard.
- [ ] Provider SDK скрыт за одним telemetry service/sink.
- [ ] Есть bounded offline queue и deterministic dedup.
- [ ] Пользователь может revoke/reset/clear telemetry state.
- [ ] Diagnostics и external telemetry остаются разными механизмами.

## Ограничения и non-goals

- advertising attribution;
- user profiling по содержимому conversations;
- отправка workspace/source code;
- remote session replay;
- автоматический upload Support Bundle;
- требование telemetry для работы приложения;
- SaaS analytics warehouse как часть локального Core;
- сбор «всего на всякий случай», потому что вдруг аналитика потом пригодится.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#72 Privacy & Telemetry Governance: consent, typed analytics events и sensitive-data boundaries](https://github.com/rkfsociety/EvoHime/issues/72)
