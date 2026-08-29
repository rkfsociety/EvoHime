# План 57.0 — Plan Artifact: versioned planning contract и явный переход Plan → Execute

Статус: предложено по [issue #37](https://github.com/rkfsociety/EvoHime/issues/37). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельный Core-owned **Plan Artifact**: versioned структурированный результат planning-фазы, который описывает предлагаемую стратегию выполнения задачи и требует явного перехода `Plan -> Execute` перед началом side-effecting реализации.

Plan не должен быть просто Markdown-файлом, который агент потом повторно читает по свободному prompt. Он является типизированным артефактом с identity, revision, acceptance criteria, dependencies, risks и provenance.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/plan-artifact.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 40.0 — Sensitive Data Guardrails: PII/secret detection и streaming redaction на model/tool boundaries.
- План 69.0 — Runtime Intervention Pipeline: Core-owned middleware for agent messages and tool boundaries.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- planning default read-only;
- PlanStep не содержит raw executable identity как authority;
- acceptance plan не отменяет tool approvals;
- required capabilities resolve через Core registry;
- execution фиксирует immutable plan revision/hash;
- material capability/risk deviation требует revalidation/replan;
- model не меняет accepted revision in-place;
- renderer не переводит status напрямую без Core command;
- sensitive paths/data проходят обычную projection policy;
- external agent control limitations не маскируются.

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

Критерии должны быть отдельной структурой:

```text
AcceptanceCriterion {
  id,
  description,
  evidence_kind,
  required,
  status?,
}
```

Примеры `evidence_kind`:

```text
TestsPass
FileState
CommandExit
ArtifactPresent
StructuredReport
HumanAccepted
ManualCheck
```

На planning этапе это expectation. Во время execution критерии связываются с фактическим evidence.

Модель не может просто написать `done=true` и считать criterion выполненным.

## Ограничения и non-goals

- хранение скрытого chain-of-thought;
- использование Markdown-файла как единственного authoritative plan state;
- автоматический запуск после генерации плана;
- превращение каждого plan step в unrestricted shell command;
- обязательная компиляция любого plan в Workflow;
- standing approval всех перечисленных side effects;
- запрет любых отклонений от плана независимо от реальности.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#37 Plan Artifact: versioned planning contract и явный переход Plan → Execute](https://github.com/rkfsociety/EvoHime/issues/37)
