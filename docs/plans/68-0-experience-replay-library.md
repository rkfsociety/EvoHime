# План 68.0 — Experience Replay Library: episodic trajectories, success/failure retrieval и context injection

Статус: предложено по [issue #48](https://github.com/rkfsociety/EvoHime/issues/48). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельную **Experience Replay Library**: хранилище эпизодического опыта выполнения задач, где сохраняются не только факты/предпочтения, а целые безопасные примеры вида `request -> plan -> action -> observation -> outcome -> score`.

Это не замена Memory Governance и не Continual Refinement.

- **Memory** хранит факты, решения, предпочтения и устойчивые знания.
- **Refinement** предлагает изменения поведения/skills/prompts.
- **Experience Replay** хранит конкретные прошлые эпизоды выполнения и позволяет подобрать несколько похожих примеров перед новой задачей.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/experience_replay_library.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./68-1-experience-replay-library.md)
- [Этап 2 — runtime-интеграция и recovery](./68-2-experience-replay-library.md)
- [Этап 3 — IPC, client projection и UI](./68-3-experience-replay-library.md)
- [Этап 4 — verification, release-evidence и закрытие](./68-4-experience-replay-library.md)

## Зависимости

### Блокирующие

- План 46.0 — Agent Role Profiles: versioned специализация, ограничения и strategy contracts.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 48.0 — Team SOP Protocols: versioned multi-agent playbooks и формальные handoff правила.
- План 66.0 — Typed Agent Handoff Contract: explicit transfer of task ownership and context.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- Experience не расширяет capabilities;
- raw credentials/secrets не сохраняются;
- retrieved experience считается untrusted contextual advice, не authority;
- model cannot self-award authoritative score;
- experience scope enforced Core-side;
- outdated/stale examples маркируются;
- unknown outcome не записывается как successful recipe;
- external/imported experiences не становятся trusted автоматически.

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

- [ ] Есть versioned ExperienceRecord/Trajectory contracts.
- [ ] Success и failure experiences имеют evidence-backed scoring.
- [ ] Запись проходит отдельный Write Gate.
- [ ] Retrieval поддерживает exact + semantic/hybrid поиск.
- [ ] Context injection bounded и progressive.
- [ ] Experience отделена от Memory и Refinement.
- [ ] Есть scope/sensitivity/retention policy.
- [ ] Повторяющиеся experiences могут служить evidence для refinement/evals.

## Ограничения и non-goals

- хранение полного chain-of-thought;
- превращение каждой выполненной задачи в experience;
- автоматическая активация новых prompt rules;
- глобальная база опыта между пользователями;
- raw transcript replay как prompt;
- доверие model-generated score без независимого evidence.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#48 Experience Replay Library: episodic trajectories, success/failure retrieval и context injection](https://github.com/rkfsociety/EvoHime/issues/48)
