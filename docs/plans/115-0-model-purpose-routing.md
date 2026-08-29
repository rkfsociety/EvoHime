# План 115.0 — Model Purpose Routing: отдельные model profiles для primary, editor, selector, summarizer и auxiliary calls

Статус: предложено по [issue #95](https://github.com/rkfsociety/EvoHime/issues/95). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Model Purpose Routing**: Core-owned слой, который назначает отдельный `ModelProfile` не только основной conversation, но и каждому внутреннему типу model call по его назначению, требованиям, стоимости и security policy.

Примеры purpose:

```text
PrimaryReasoning
CodeEditing
ArchitectureReasoning
ToolSelection
TeamSelection
ContextSelection
Summarization
Compaction
CommitMessage
Review
Judge
Refinement
Simulation
```

Система должна объединить разрозненные model-purpose настройки в один versioned routing contract, не заставляя каждую subsystem самостоятельно хранить `weak_model`, `editor_model`, `selector_model` и другие исторически неизбежные имена.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/model-purpose-routing.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 39.0 — Structured Response Contract: schema-first ответы модели с provider/tool fallback.
- План 42.0 — Model Resilience Policy: retry, fallback и provider-safe request adaptation.
- План 67.0 — Schema-Driven Agent Configuration: Core-owned schemas для agent/conversation settings.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 36.0 — Agent Benchmark Matrix: многократные model/strategy evals и regression tracking.
- План 46.0 — Agent Role Profiles: versioned специализация, ограничения и strategy contracts.
- План 59.0 — Incremental Change Protocol: safe requirement-delta pipeline для существующих репозиториев.
- План 71.0 — Workflow Optimization Lab: offline search и benchmark-driven улучшение agent workflows.
- План 83.0 — Reasoning Operator Library: typed Generate/Review/Revise/Ensemble primitives для agent workflows.
- План 105.0 — Prompt Cache Planner: stable context segments, provider-aware cache hints и reuse metrics.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Subsystem requests model call
  -> ModelCallPurpose + requirements
  -> Model Purpose Router
  -> allowed purpose policy
  -> candidate ModelProfiles
  -> compatibility / trust / budget checks
  -> resolved ModelProfile
  -> Model Resilience Policy
  -> Model Gateway
```

Routing выбирает **первичный профиль для purpose**.

Retry/fallback после выбора остаётся обязанностью Model Resilience Policy и не дублируется здесь.

### Безопасность

- routing выбирает только registered ModelProfiles;
- purpose tool policy только сужает grants;
- `NoTools` cannot be overridden by model output;
- provider/sensitivity checks выполняются для каждого internal call;
- raw credentials не входят в routing context;
- fallback не реализуется скрыто самим router-ом, а проходит Resilience Policy;
- user override не подменяет hard capability/security constraints;
- internal selector/summarizer calls не попадают как user-authored messages в conversation;
- routing config pinned для active run/model call.

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

- [ ] Есть stable Core-owned ModelCallPurpose registry.
- [ ] Есть versioned ModelPurposeRoutingPolicy.
- [ ] Internal subsystems запрашивают модель через purpose вместо собственных raw model-name settings.
- [ ] Purpose задаёт requirements, tool ceiling и context policy.
- [ ] Routing проверяет model capabilities, trust/locality и budget.
- [ ] Retry/fallback остаётся отдельной Model Resilience Policy.
- [ ] Exact purpose/profile фиксируются в model-call provenance и usage.
- [ ] UI позволяет настраивать основные purpose routes через ModelProfile refs.

## Ограничения и non-goals

- непрозрачный ML-router, автоматически выбирающий любую модель на рынке;
- скрытая смена provider вопреки user/data policy;
- динамическое расширение tool grants из-за выбранной модели;
- отдельная model database внутри каждой subsystem;
- обязательная отдельная модель для каждого purpose;
- автоматический выбор самой дорогой модели ради предполагаемого quality gain;
- смешивание routing и retry/fallback semantics.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#95 Model Purpose Routing: отдельные model profiles для primary, editor, selector, summarizer и auxiliary calls](https://github.com/rkfsociety/EvoHime/issues/95)
