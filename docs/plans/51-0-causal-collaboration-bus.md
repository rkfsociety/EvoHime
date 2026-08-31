# План 51.0 — Causal Collaboration Bus: typed pub/sub для team agents поверх child mailbox

Статус: предложено по [issue #31](https://github.com/rkfsociety/EvoHime/issues/31). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Causal Collaboration Bus**: Core-owned typed message layer для ограниченного обмена событиями между участниками одной зарегистрированной team session.

Это расширение существующего parent-child mailbox, а не его замена. Parent-scoped retained child messaging остаётся базовым безопасным механизмом; Collaboration Bus добавляет controlled peer routing только внутри заранее определённого roster/TeamProtocol.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатный policy/routing слой —
`crates/evohime-core/src/causal_collaboration_bus.rs`, но transport и storage
обязаны расширять `retained_child.rs`/`retained_child_store.rs`, а roster и
разрешённые routes — использовать `agent_role_profiles.rs` и
`team_sop_protocols.rs` с их durable stores. Второй mailbox, параллельный
roster или model-supplied sender identity запрещены.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./51-1-causal-collaboration-bus.md)
- [Этап 2 — runtime-интеграция и recovery](./51-2-causal-collaboration-bus.md)
- [Этап 3 — IPC, client projection и UI](./51-3-causal-collaboration-bus.md)
- [Этап 4 — verification, release-evidence и закрытие](./51-4-causal-collaboration-bus.md)

## Зависимости

### Блокирующие

- реализованные Retained Child Contexts, Agent Role Profiles и Team SOP
  Protocols contracts из `../architecture.md`;
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- Artifact Handoff Registry (план 56) для крупных typed deliverables; до него
  bus передаёт только bounded inline payload и существующие ArtifactStore refs.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- sender identity Core-derived;
- routing только внутри session roster;
- subscriptions не расширяют artifact/grant authority;
- payload проходит size/schema/sensitivity checks;
- Secret передаётся через refs по ArtifactStore policy;
- model cannot forge another role address;
- peer messaging можно полностью запретить per TeamProtocol;
- message kind не является security approval;
- удалённый/expired participant больше не является valid destination.

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

- [ ] Есть typed CollaborationMessage envelope.
- [ ] Sender identity и routing Core-owned.
- [ ] Есть protocol-scoped subscriptions и peer routes.
- [ ] Есть causation/correlation/sequence metadata.
- [ ] Inbox bounded и имеет backpressure semantics.
- [ ] Durable significant messages recoverable/deduplicated.
- [ ] Layer переиспользует/расширяет child mailbox, а не дублирует его без причины.
- [ ] Artifact authority не расширяется фактом подписки.

## Ограничения и non-goals

- глобальная сеть агентов;
- arbitrary cross-project peer chat;
- social/chat UI для агентов;
- broadcast всего transcript всем участникам;
- использование message bus вместо formal artifact/report contracts;
- обход workflow/child grants через адресацию.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#31 Causal Collaboration Bus: typed pub/sub для team agents поверх child mailbox](https://github.com/rkfsociety/EvoHime/issues/31)

## Результат ревью 2026-09-01

- Dependency map привязан к реализованным retained-child, role-profile и
  TeamProtocol surfaces; нерелевантная зависимость от Skill Trust Pipeline
  удалена.
- Зафиксировано переиспользование mailbox/store и bounded degradation до
  появления semantic Artifact Handoff Registry.
