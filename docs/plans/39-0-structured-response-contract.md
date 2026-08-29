# План 39.0 — Structured Response Contract: schema-first ответы модели с provider/tool fallback

Статус: предложено по [issue #19](https://github.com/rkfsociety/EvoHime/issues/19). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime единый **Structured Response Contract** для model calls, где вызывающий слой задаёт versioned output schema, а Model Gateway выбирает подходящую стратегию получения валидного структурированного результата.

Это нужно для случаев, где ответ модели является не пользовательским prose, а частью машинного протокола:

- child reports;
- workflow node outputs;
- tool selection;
- refinement candidates;
- planner/critic results;
- classification/routing;
- extraction;
- UI-generated structured artifacts.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/structured-response-contract.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 24.0 — Agent Skills: registry, SKILL.md и progressive disclosure.
- План 37.0 — Agent Middleware Pipeline: typed hooks вокруг model/tool execution.
- План 38.0 — Adaptive Tool Catalog: dynamic selection и deferred tool schemas.
- План 42.0 — Model Resilience Policy: retry, fallback и provider-safe request adaptation.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

Высшие слои не должны знать provider-specific синтаксис structured output.

```text
Caller
 -> ResponseContract
 -> Model Gateway
 -> choose strategy
 -> provider-native strict schema OR synthetic output tool
 -> validate
 -> typed result / typed error
```

### Безопасность

- synthetic output tool не является capability и не выполняет side effects;
- model не может подменить contract id/schema;
- response schema приходит от Core/caller, а не из model output;
- schema size/depth bounded;
- validation проводится локально;
- retry diagnostics redacted;
- provider fallback не снимает strictness contract;
- malformed output не проходит дальше как trusted typed data.

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

- [ ] Есть versioned ResponseContract.
- [ ] Gateway поддерживает provider-native и synthetic-tool strategies.
- [ ] Есть Auto strategy на основании model capabilities.
- [ ] Все outputs проходят Core-side schema validation.
- [ ] Ошибки typed и различают parse/validation/multiple/unsupported.
- [ ] Repair retries bounded.
- [ ] Contract version/hash фиксируется в run provenance.
- [ ] Provider fallback может менять transport strategy без изменения caller contract.

## Ограничения и non-goals

- заменять обычные пользовательские текстовые ответы structured JSON;
- доверять provider validation без локальной проверки;
- использовать executable tools как surrogate output channel;
- бесконечный repair loop;
- runtime-generated arbitrary schemas без size/depth limits;
- provider-specific response payloads в workflow/child contracts.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#19 Structured Response Contract: schema-first ответы модели с provider/tool fallback](https://github.com/rkfsociety/EvoHime/issues/19)
