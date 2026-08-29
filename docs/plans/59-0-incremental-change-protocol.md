# План 59.0 — Incremental Change Protocol: safe requirement-delta pipeline для существующих репозиториев

Статус: предложено по [issue #39](https://github.com/rkfsociety/EvoHime/issues/39). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime формальный **Incremental Change Protocol** для работы с уже существующим репозиторием: новая задача трактуется как изменение относительно текущего baseline, а не как повод заново «спроектировать проект» и переписать половину дерева файлов.

Protocol должен связывать:

```text
Requirement Delta
 -> Baseline Snapshot
 -> Impact Analysis
 -> Change Plan
 -> Patch/Implementation
 -> Review
 -> Tests/Evidence
 -> Artifact/Summary Update
```

и быть resumable через существующий workflow/checkpoint runtime.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/incremental-change-protocol.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 57.0 — Plan Artifact: versioned planning contract и явный переход Plan → Execute.
- План 58.0 — Workspace State Checkpoints: безопасный rollback файлов отдельно от task history.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 38.0 — Adaptive Tool Catalog: dynamic selection и deferred tool schemas.
- План 84.0 — Output Guardrail Pipeline: semantic validators, transforms и bounded correction loops.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- baseline и file identity Core-derived;
- plan не расширяет tool grants;
- path scope проходит обычные workspace permissions;
- stale/conflicting plan не применяется вслепую;
- unrelated user dirty changes сохраняются;
- patch provenance immutable;
- external/imported plan не получает trust автоматически;
- destructive removal всё ещё требует обычной policy/approval.

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

- [ ] Есть explicit RequirementDelta + RepositoryBaseline.
- [ ] Есть ImpactAnalysis и versioned ChangePlan.
- [ ] Реализация привязана к plan items и before/after fingerprints.
- [ ] Workspace drift проверяется до применения stale plan.
- [ ] Scope drift видим и может gate-иться policy.
- [ ] Review/tests используют exact implementation revision/fingerprint.
- [ ] Checkpoint сохраняет incremental progress.
- [ ] После завершения обновляется change/code summary и artifact lineage.

## Ограничения и non-goals

- обязательный тяжёлый PRD/architecture pipeline для каждого bugfix;
- переписывание существующего проекта «для чистоты»;
- semantic merge любых конфликтов без review;
- автоматический commit/push;
- замена Git;
- доверие старым summaries вместо чтения актуального кода.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#39 Incremental Change Protocol: safe requirement-delta pipeline для существующих репозиториев](https://github.com/rkfsociety/EvoHime/issues/39)
