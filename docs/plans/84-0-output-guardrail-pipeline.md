# План 84.0 — Output Guardrail Pipeline: semantic validators, transforms и bounded correction loops

Статус: предложено по [issue #64](https://github.com/rkfsociety/EvoHime/issues/64). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Output Guardrail Pipeline**: versioned Core-owned цепочку проверок результата задачи/роли перед тем, как результат станет accepted output, artifact handoff или context для следующего шага.

Pipeline должен поддерживать два класса проверок:

1. **deterministic validators/transforms** для формальных требований;
2. **model-based semantic validators** для требований, которые трудно выразить схемой или кодом.

При исправимой ошибке validator возвращает структурированную обратную связь исполнителю и разрешает bounded correction loop.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/output-guardrail-pipeline.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 39.0 — Structured Response Contract: schema-first ответы модели с provider/tool fallback.
- План 40.0 — Sensitive Data Guardrails: PII/secret detection и streaming redaction на model/tool boundaries.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 57.0 — Plan Artifact: versioned planning contract и явный переход Plan → Execute.
- План 69.0 — Runtime Intervention Pipeline: Core-owned middleware for agent messages and tool boundaries.
- План 97.0 — Model Edit Protocol Registry: строгие patch/search-replace стратегии и repair feedback.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- guardrail registry Core-owned;
- imported content не исполняет arbitrary validator code;
- model judge не имеет tools/credentials;
- guardrail не расширяет capabilities;
- semantic judge не может фальсифицировать Core evidence;
- transform не изменяет security metadata/provenance;
- stale artifact/evidence не принимается как current без policy;
- Secret values redacted из feedback/UI;
- retry loop bounded.

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

- [ ] Есть versioned OutputGuardrail/Policy contracts.
- [ ] Поддержаны ordered deterministic и model-based checks.
- [ ] Поддержан validated transform stage.
- [ ] Retry/correction loop bounded и durable.
- [ ] Guardrail result имеет provenance/evidence.
- [ ] Acceptance привязана к exact output/artifact revision.
- [ ] Security approvals и real-effect evidence остаются отдельными слоями.
- [ ] UI показывает validation state без смешения классов доказательств.

## Ограничения и non-goals

- замена JSON/schema validation;
- выполнение arbitrary Python/JS guardrails из workspace;
- бесконечные self-correction loops;
- использование LLM judge для проверяемых кодом условий;
- считать semantic judge доказательством реального side effect;
- скрытое изменение model output без provenance.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#64 Output Guardrail Pipeline: semantic validators, transforms и bounded correction loops](https://github.com/rkfsociety/EvoHime/issues/64)
