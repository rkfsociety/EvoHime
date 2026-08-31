# План 91.0 — Architect-Editor Model Pipeline: раздельные reasoning и code-editing фазы

Статус: предложено по [issue #71](https://github.com/rkfsociety/EvoHime/issues/71). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Architect-Editor Model Pipeline**: versioned orchestration profile, позволяющий разделить сложную coding-задачу на две model-фазы:

```text
User objective
 -> Architect / Reasoner
 -> typed EditIntent
 -> Editor / Implementer
 -> revision-safe mutations
 -> validation
```

Architect отвечает за понимание задачи, constraints и способ изменения. Editor получает уже сформулированное намерение и переводит его в конкретные file operations.

Это не новый security role и не замена Agent Role Profiles/child agents. Это **внутри одного coding turn/run** способ использовать разные model strengths для разных стадий.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/architect_editor_model_pipeline.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./91-1-architect-editor-model-pipeline.md)
- [Этап 2 — runtime-интеграция и recovery](./91-2-architect-editor-model-pipeline.md)
- [Этап 3 — IPC, client projection и UI](./91-3-architect-editor-model-pipeline.md)
- [Этап 4 — verification, release-evidence и закрытие](./91-4-architect-editor-model-pipeline.md)

## Зависимости

### Блокирующие

- План 36.0 — Agent Benchmark Matrix: многократные model/strategy evals и regression tracking.
- Tool Simulation Runtime v1 из `../architecture.md`.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- Model Resilience Policy v1 из `docs/architecture.md`.
- План 71.0 — Workflow Optimization Lab: offline search и benchmark-driven улучшение agent workflows.
- План 83.0 — Reasoning Operator Library: typed Generate/Review/Revise/Ensemble primitives для agent workflows.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- Architect output не является capability grant;
- Editor writes проходят Revision-Safe Workspace Files;
- models выбираются только из allowed provider/model profiles;
- sensitive context projection проверяется отдельно для каждой phase/provider;
- workspace drift проверяется между phases;
- raw hidden reasoning не сохраняется как required execution state;
- retry loops bounded;
- editor cannot widen expected/granted path scope silently.

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

- [ ] Есть versioned ModelPhasePipeline contract.
- [ ] Architect output оформлен как typed EditIntent.
- [ ] Editor phase может использовать отдельный ModelProfile/edit protocol.
- [ ] Same-model и separate-model modes поддерживаются.
- [ ] Workspace drift проверяется между phases.
- [ ] Failure routing/retries bounded и typed.
- [ ] Все actual writes проходят существующий Core mutation/security boundary.
- [ ] UI/metrics различают architect/editor purpose.

## Ограничения и non-goals

- раскрытие chain-of-thought architect-а;
- обязательные две model calls для каждого edit;
- превращение architect/editor в отдельную multi-agent team без необходимости;
- выдача editor новых filesystem/tool grants из model output;
- автоматический выбор самого дорогого model pair;
- обход Plan Artifact/human review для risky изменений.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#71 Architect-Editor Model Pipeline: раздельные reasoning и code-editing фазы](https://github.com/rkfsociety/EvoHime/issues/71)
