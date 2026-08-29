# План 113.0 — Policy-Aware Tool Result Cache: freshness, provenance и safe reuse read-only calls

Статус: предложено по [issue #93](https://github.com/rkfsociety/EvoHime/issues/93). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Policy-Aware Tool Result Cache**: Core-owned механизм повторного использования результатов только тех tool calls, для которых это явно безопасно и семантически корректно.

Главный принцип:

> Cache является оптимизацией чтения, а не способом симулировать выполнение действия.

По умолчанию tool **не cacheable**. Поддержка включается только через trusted tool/provider metadata и ограничивается read-only/idempotent observation calls в первом этапе.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/policy_aware_tool_result_cache.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./113-1-policy-aware-tool-result-cache.md)
- [Этап 2 — runtime-интеграция и recovery](./113-2-policy-aware-tool-result-cache.md)
- [Этап 3 — IPC, client projection и UI](./113-3-policy-aware-tool-result-cache.md)
- [Этап 4 — verification, release-evidence и закрытие](./113-4-policy-aware-tool-result-cache.md)

## Зависимости

### Блокирующие

- План 38.0 — Adaptive Tool Catalog: dynamic selection и deferred tool schemas.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 40.0 — Sensitive Data Guardrails: PII/secret detection и streaming redaction на model/tool boundaries.
- План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection.
- План 105.0 — Prompt Cache Planner: stable context segments, provider-aware cache hints и reuse metrics.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- default `Never`;
- cache declaration только trusted registry/provider metadata;
- mutating tools Never в MVP;
- cache reuse только внутри compatible authority/account/workspace scope;
- raw credential values отсутствуют в key/storage/log;
- cached evidence несёт original observed_at/provenance;
- `RequireFresh` bypasses cache;
- cache cannot satisfy approval or simulate effect execution;
- schema/tool revision drift invalidates entry;
- untrusted skill/workflow не может пометить произвольный side-effecting tool cacheable.

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

- [ ] Tools/actions имеют explicit trusted cacheability metadata.
- [ ] Default cacheability = Never.
- [ ] Cache key учитывает version/schema/resource/account/policy context.
- [ ] Есть TTL/freshness и explicit `RequireFresh`.
- [ ] Cached results сохраняют source provenance/observed time.
- [ ] Mutating tools не используют result cache в MVP.
- [ ] Workspace/provider/credential drift инвалидирует entries.
- [ ] Sensitive cache storage регулируется policy.
- [ ] Есть bounded storage/eviction и optional single-flight.

## Ограничения и non-goals

- кеширование side-effecting execution вместо idempotency protocol;
- использование cache как source of truth;
- model-defined arbitrary cache key functions;
- бесконечный persistent storage;
- cross-user/account cache sharing;
- считать stale cached external state актуальным без marker/policy;
- заменять Tool Simulation Runtime.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#93 Policy-Aware Tool Result Cache: freshness, provenance и safe reuse read-only calls](https://github.com/rkfsociety/EvoHime/issues/93)
