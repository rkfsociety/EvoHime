# План 54.0 — Human Work Items: пользователь как полноценный участник workflow/team, а не только approval

Статус: предложено по [issue #34](https://github.com/rkfsociety/EvoHime/issues/34). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Human Work Item**: durable typed задачу, которую agent/workflow/team может передать человеку для содержательной работы и затем продолжить выполнение после получения schema-valid результата.

Human Work Item отличается от security approval.

Approval отвечает:

> Разрешить ли конкретный side effect?

Human Work Item отвечает:

> Выполни часть работы, выбери вариант, внеси правки, проверь результат или предоставь недостающий deliverable.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/human-work-items.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./54-1-human-work-items.md)
- [Этап 2 — runtime-интеграция и recovery](./54-2-human-work-items.md)
- [Этап 3 — IPC, client projection и UI](./54-3-human-work-items.md)
- [Этап 4 — verification, release-evidence и закрытие](./54-4-human-work-items.md)

## Зависимости

### Блокирующие

- План 45.0 — External Coding Agent Adapter: подключение Codex/Claude/Gemini-подобных executors через typed protocol.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 34.0 — Event Trigger Runtime: безопасный запуск workflow по внешним событиям.
- План 35.0 — Invocation Presets: version-pinned шаблоны запусков без копирования секретов.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- work item не является capability grant;
- raw secrets не показываются без существующей reveal policy;
- response schema Core-owned;
- sender/requester identity Core-derived;
- agent не может сфальсифицировать human submission;
- human response не становится shell/tool identity;
- attachments проходят обычную file/artifact validation;
- expired/cancelled work item не принимает late result без explicit reopen/new revision.

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

- [ ] Есть durable HumanWorkItem contract/state machine.
- [ ] Human work отделён от security approvals.
- [ ] Responses могут быть typed/schema-validated.
- [ ] Есть accept/revise/replan semantics.
- [ ] Human может занимать compatible Team Role slot.
- [ ] Work items переживают restart и имеют Inbox UI.
- [ ] Human response не расширяет runtime capabilities.

## Ограничения и non-goals

- multi-user SaaS assignment system;
- crowdsourcing задач внешним людям;
- HR/team management;
- автоматическое выполнение human work AI-моделью после timeout;
- замена approvals;
- выдача пользователю скрытых agent credentials/tools через HumanWorkItem.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#34 Human Work Items: пользователь как полноценный участник workflow/team, а не только approval](https://github.com/rkfsociety/EvoHime/issues/34)
