# План 87.0 — Standing Approval Profiles: scoped auto-approval по action class, path, risk и времени

Статус: предложено по [issue #67](https://github.com/rkfsociety/EvoHime/issues/67). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Standing Approval Profiles**: Core-owned механизм, позволяющий пользователю заранее разрешить ограниченный класс повторяющихся действий в конкретном scope, чтобы агент не запрашивал одно и то же подтверждение десятки раз, сохраняя при этом явные границы, audit и возможность мгновенного отзыва.

Это не режим «разрешить всё». Напротив, задача механизма — заменить раздражающее approve-by-clicking на **узкие формальные grants**, а не на выключение security model.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/standing-approval-profiles.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 76.0 — Safe UI Extension Framework: declarative pages, panels и themes без renderer authority.
- План 80.0 — Project Instruction Stack: conditional rules, AGENTS.md compatibility и deterministic precedence.
- План 95.0 — Team Coordination Strategies: pluggable selector, round-robin, swarm и graph routing.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
StandingApprovalProfile {
  id,
  version,
  name,
  enabled,
  subject_scope,
  action_rules[],
  created_by,
  created_at,
  expires_at?,
  max_uses?,
  revision,
  content_hash
}
```

`subject_scope`:

```text
UserGlobal
Workspace
WorkspaceSet
Conversation
Goal
WorkflowDefinition
WorkflowRun
AgentRole
```

Чем шире scope, тем консервативнее допустимые action classes.

### Безопасность

- модель не создаёт/редактирует standing approvals;
- standing profile не расширяет capability grants;
- risk/effect classification Core-owned;
- arbitrary shell strings не получают широкого wildcard approval;
- paths/domains canonicalized;
- hard-deny critical effects остаются fail-closed;
- background execution требует отдельной applicability policy;
- plan/workflow import не может скрыто создавать permission;
- revocation/versioning audit-able;
- unknown outcome не приводит к implicit retry.

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

- [ ] Есть versioned StandingApprovalProfile/Rule contracts.
- [ ] Rules scoped по subject, action class, resource и risk.
- [ ] Core, а не модель, классифицирует effects/risk и выполняет matching.
- [ ] Standing approval не расширяет grants и не заменяет ExecutionPolicy.
- [ ] Есть duration/use limits и foreground/background semantics.
- [ ] Пользователь может понять, почему action прошёл без prompt.
- [ ] Persistent profiles можно безопасно revoke/edit.
- [ ] Critical/hard-deny effects fail-closed по Core policy.

## Ограничения и non-goals

- кнопка «разрешить абсолютно всё навсегда»;
- отключение Core capability/ExecutionPolicy checks;
- wildcard auto-approval произвольного shell;
- автоматическое расширение scope по просьбе модели;
- implicit approval внешних destructive/publish actions;
- использование accepted Plan как универсального разрешения всех будущих effects.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#67 Standing Approval Profiles: scoped auto-approval по action class, path, risk и времени](https://github.com/rkfsociety/EvoHime/issues/67)
