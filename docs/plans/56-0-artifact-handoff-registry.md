# План 56.0 — Artifact Handoff Registry: typed deliverables, lineage и freshness для multi-agent работы

Статус: предложено по [issue #36](https://github.com/rkfsociety/EvoHime/issues/36). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Расширить существующий ArtifactStore слоем **Artifact Handoff Registry**: формализовать проектные deliverables, их версии, producer/consumer relationships, lineage, acceptance и freshness, чтобы роли/workflows обменивались конкретными артефактами вместо пересылки больших фрагментов истории.

Это не новый файловый storage. ArtifactStore остаётся владельцем content bytes/refs. Новый registry описывает **семантическую роль артефакта в проектном процессе**.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/artifact-handoff-registry.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./56-1-artifact-handoff-registry.md)
- [Этап 2 — runtime-интеграция и recovery](./56-2-artifact-handoff-registry.md)
- [Этап 3 — IPC, client projection и UI](./56-3-artifact-handoff-registry.md)
- [Этап 4 — verification, release-evidence и закрытие](./56-4-artifact-handoff-registry.md)

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- План 27.0 — Retained Child Contexts: mailbox и повторное использование child agents.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 30.0 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей.
- План 36.0 — Agent Benchmark Matrix: многократные model/strategy evals и regression tracking.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Draft
Produced
UnderReview
Accepted
NeedsRevision
Superseded
Stale
Rejected
```

`Produced` означает, что producer сформировал deliverable. `Accepted` означает, что нужная acceptance policy выполнена.

Нельзя считать новый файл автоматически accepted только потому, что agent записал его в workspace.

### Безопасность

- ArtifactStore остаётся authority по content/sensitivity;
- handoff не расширяет права consumer;
- Secret content передаётся refs/projections;
- artifact type/contract Core-owned/versioned;
- model не может подделать producer identity;
- lineage refs валидируются и cycles запрещены;
- external/imported artifact не становится trusted/Accepted без validation;
- stale artifact не используется как fresh evidence без explicit policy.

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

- [ ] ArtifactStore дополнен semantic ProjectArtifact registry.
- [ ] Есть versioned artifact contracts/types.
- [ ] Есть immutable revisions и lineage DAG.
- [ ] Handoffs typed и имеют producer/consumer identity.
- [ ] Есть acceptance/revision lifecycle.
- [ ] Artifact freshness может зависеть от workspace/parent fingerprints.
- [ ] Agent context использует refs/projections вместо полного transcript.
- [ ] UI показывает revisions/status/lineage/handoffs.

## Ограничения и non-goals

- замена Git;
- отдельное хранилище копий всех файлов проекта;
- гарантия автоматического понимания семантической зависимости любого документа;
- публичная knowledge graph;
- автоматический trust импортированных artifacts;
- превращение каждого model message в project deliverable.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#36 Artifact Handoff Registry: typed deliverables, lineage и freshness для multi-agent работы](https://github.com/rkfsociety/EvoHime/issues/36)
