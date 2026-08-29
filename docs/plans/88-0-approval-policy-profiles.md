# План 88.0 — Approval Policy Profiles: granular standing decisions без blanket auto-approve

Статус: предложено по [issue #68](https://github.com/rkfsociety/EvoHime/issues/68). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime versioned **Approval Policy Profiles**: Core-owned правила, которые заранее определяют, какие классы уже разрешённых действий можно выполнять без отдельного popup, а какие всегда требуют явного решения пользователя.

Задача не в том, чтобы создать «YOLO mode». Наоборот: убрать раздражение от десятков одинаковых подтверждений, **не превращая удобство в отключение security model**.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/approval_policy_profiles.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./88-1-approval-policy-profiles.md)
- [Этап 2 — runtime-интеграция и recovery](./88-2-approval-policy-profiles.md)
- [Этап 3 — IPC, client projection и UI](./88-3-approval-policy-profiles.md)
- [Этап 4 — verification, release-evidence и закрытие](./88-4-approval-policy-profiles.md)

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- План 50.0 — Memory Governance: typed memory, evidence gates, reinforcement и retention policy.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 36.0 — Agent Benchmark Matrix: многократные model/strategy evals и regression tracking.
- План 49.0 — Resumable Conversation Event Log: cursor-based history, live sync и reconnect без дублей.
- План 68.0 — Experience Replay Library: episodic trajectories, success/failure retrieval и context injection.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- approval profile не расширяет grants;
- system hard requirements имеют приоритет;
- model risk hint не authoritative;
- rules Core-validated/versioned;
- imported content не создаёт standing allow;
- broad rules требуют explicit user action;
- child inheritance не widening;
- expired/revoked rule прекращает работать немедленно согласно snapshot policy;
- Secret fields masked в audit/UI;
- policy edit не считается обычным low-risk tool action.

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

- [ ] Есть versioned ApprovalPolicyProfile.
- [ ] Rules матчат stable Core action/risk metadata и deterministic constraints.
- [ ] Core, а не модель, является authority по approval requirement.
- [ ] Есть run/conversation/workspace scopes и expiration.
- [ ] Hard approval requirements нельзя снять profile-ом.
- [ ] Пользователь может создать bounded `allow similar` из approval dialog.
- [ ] Child/headless execution не превращает отсутствие prompt в blanket approval.
- [ ] Все решения имеют auditable matched-rule provenance.

## Ограничения и non-goals

- YOLO/approve-everything как рекомендованный режим;
- отключение capability/security/ExecutionPolicy;
- arbitrary code predicates;
- model-authored standing permissions;
- автоматический trust любого command на основании LLM-классификации;
- перенос approval profiles между пользователями с автоматической активацией;
- считать Git/version control достаточной защитой от destructive side effects.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#68 Approval Policy Profiles: granular standing decisions без blanket auto-approve](https://github.com/rkfsociety/EvoHime/issues/68)
