# План 105.0 — Prompt Cache Planner: stable context segments, provider-aware cache hints и reuse metrics

Статус: предложено по [issue #85](https://github.com/rkfsociety/EvoHime/issues/85). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Prompt Cache Planner**: Core-owned слой, который строит model request так, чтобы стабильные части контекста имели предсказуемое содержимое/порядок, могли переиспользоваться provider-side prompt caching и не инвалидировались из-за случайного rearrangement динамических данных.

Это performance/cost optimization. Cache никогда не становится source of truth и не меняет security semantics контекста.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/prompt_cache_planner.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./105-1-prompt-cache-planner.md)
- [Этап 2 — runtime-интеграция и recovery](./105-2-prompt-cache-planner.md)
- [Этап 3 — IPC, client projection и UI](./105-3-prompt-cache-planner.md)
- [Этап 4 — verification, release-evidence и закрытие](./105-4-prompt-cache-planner.md)

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 38.0 — Adaptive Tool Catalog: dynamic selection и deferred tool schemas.
- План 67.0 — Schema-Driven Agent Configuration: Core-owned schemas для agent/conversation settings.
- План 75.0 — Typed Context References: адресные @refs на файлы, diff, diagnostics, terminal и artifacts.
- План 86.0 — Semantic Repository Map: symbol graph и token-budgeted контекст большого репозитория.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- cache planner не меняет instruction priority;
- cache не расширяет context grants;
- sensitivity/provider trust проверяется до send;
- cache key использует exact revisions/policy versions;
- stale cached projection не используется как current resource;
- keepalive off by default и budget-controlled;
- raw secrets не попадают в diagnostics/cache labels;
- provider caching limitations честно отражаются.

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

- [ ] Model context имеет explicit hashed PromptSegments.
- [ ] Stable/dynamic segments сериализуются deterministic.
- [ ] Provider cache capabilities описаны через profile, не provider-name branches.
- [ ] Exact revisions/policy versions участвуют в invalidation.
- [ ] Cache plan не меняет instruction/security semantics.
- [ ] Usage умеет показывать measured cache metrics.
- [ ] Keepalive отсутствует по умолчанию и bounded при явном включении.
- [ ] Benchmark/evals покрывают cache reuse/invalidation.

## Ограничения и non-goals

- собственный remote prompt-cache сервер;
- хранение raw prompts в отдельной небезопасной cache DB;
- принудительный keepalive;
- увеличение context только ради cache hits;
- обход sensitivity rules;
- гарантия одинаковой caching semantics у всех providers;
- изменение system/user instruction hierarchy ради оптимизации.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#85 Prompt Cache Planner: stable context segments, provider-aware cache hints и reuse metrics](https://github.com/rkfsociety/EvoHime/issues/85)
