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

Plan Artifact обязан консолидировать существующие `plan.rs`,
`plan_context.rs`, `plan_review.rs`, TaskCheckpoint и plan-related event
projection. Новый `plan_artifact.rs` допустим как authoritative versioned
aggregate, но старые `TaskPlanSpec`/review paths должны либо адаптироваться к
нему, либо остаться явно read-only inputs; второй mutable plan status запрещён.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./57-1-plan-artifact.md)
- [Этап 2 — runtime-интеграция и recovery](./57-2-plan-artifact.md)
- [Этап 3 — IPC, client projection и UI](./57-3-plan-artifact.md)
- [Этап 4 — verification, release-evidence и закрытие](./57-4-plan-artifact.md)

## Зависимости

### Блокирующие

- реализованные TaskCheckpoint v1, capability registry, event journal и
  Sensitive Data Guardrails v1 из канонической архитектуры;
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

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

Acceptance criteria внутри plan должны быть отдельной структурой и
подтверждаться независимо от свободного текста модели:

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

Критерии направления из issue #37:

- [ ] Есть versioned PlanArtifact с immutable revisions.
- [ ] Planning и Execution являются явными режимами/переходами.
- [ ] Planning default не выполняет обычные mutating effects.
- [ ] Plan содержит steps, assumptions, risks и acceptance criteria.
- [ ] Core валидирует plan перед acceptance.
- [ ] ExecutePlan фиксирует exact revision/hash и создаёт execution snapshot.
- [ ] TaskCheckpoint отслеживает фактическое выполнение отдельно от plan.
- [ ] Material deviations имеют явный re-plan path.
- [ ] Plan acceptance не заменяет security approvals.

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

## Результат ревью 2026-09-01

- План сопоставлен с действующими plan/review/TaskCheckpoint surfaces; добавлен
  обязательный migration/coexistence contract без второго mutable plan state.
- Sensitive Data Guardrails переведены из опциональной ссылки в обязательную
  каноническую boundary для plan/evidence projection.
