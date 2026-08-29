# План 48.0 — Team SOP Protocols: versioned multi-agent playbooks и формальные handoff правила

Статус: предложено по [issue #28](https://github.com/rkfsociety/EvoHime/issues/28). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime versioned **Team SOP Protocol**: формальный playbook для совместной работы нескольких специализированных ролей над одной задачей.

SOP описывает не просто последовательность workflow nodes, а **организационный контракт команды**:

- какие роли участвуют;
- за какой результат отвечает каждая роль;
- какие фазы проходит команда;
- какие deliverables передаются между ролями;
- что запускает следующую фазу;
- где разрешён parallel work;
- кто и что обязан review/revise;
- когда команда считается завершившей работу;
- как распределяются grants и budgets.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/team_sop_protocols.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./48-1-team-sop-protocols.md)
- [Этап 2 — runtime-интеграция и recovery](./48-2-team-sop-protocols.md)
- [Этап 3 — IPC, client projection и UI](./48-3-team-sop-protocols.md)
- [Этап 4 — verification, release-evidence и закрытие](./48-4-team-sop-protocols.md)

## Зависимости

### Блокирующие

- План 46.0 — Agent Role Profiles: versioned специализация, ограничения и strategy contracts.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- protocol не регистрирует executable tools;
- role grants ограничиваются parent/workflow/role ceiling;
- peer routing разрешается только по protocol roster/routes;
- handoff не передаёт raw secret/transcript по умолчанию;
- imported protocol не активируется без обычной validation;
- review/acceptance не является security approval, если effect требует отдельного approval;
- protocol version pinned на session.

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

- [ ] Есть versioned TeamProtocol contract.
- [ ] Participants ссылаются на versioned Agent Role Profiles.
- [ ] Phases/handoffs/exit criteria формализованы.
- [ ] Есть bounded review/revise loop.
- [ ] TeamProtocol исполняется поверх существующего workflow/child runtime.
- [ ] TeamSession имеет immutable protocol snapshot.
- [ ] UI показывает roster/phases/handoffs/progress.
- [ ] SOP не расширяет capabilities.

## Ограничения и non-goals

- отдельный второй workflow engine;
- свободный неограниченный group chat агентов;
- SaaS organization/team management;
- автоматическая генерация новых executable role implementations;
- бесконечные review loops;
- замена security approvals командным consensus.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#28 Team SOP Protocols: versioned multi-agent playbooks и формальные handoff правила](https://github.com/rkfsociety/EvoHime/issues/28)
