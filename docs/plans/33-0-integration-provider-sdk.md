# План 33.0 — Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures

Статус: предложено по [issue #13](https://github.com/rkfsociety/EvoHime/issues/13). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Integration Provider SDK/contract** для системного описания внешних сервисов: GitHub, Google, Slack, Linear и т.п. Один provider package должен описывать authentication, доступные actions, trigger/webhook capabilities, schemas, risk metadata и тестовые fixtures.

Это не замена MCP и не новый unrestricted plugin runtime. Это единый Core-owned способ представить внешнюю интеграцию так, чтобы UI, workflow registry, credentials, approvals и diagnostics работали согласованно.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/integration-provider-sdk.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./33-1-integration-provider-sdk.md)
- [Этап 2 — runtime-интеграция и recovery](./33-2-integration-provider-sdk.md)
- [Этап 3 — IPC, client projection и UI](./33-3-integration-provider-sdk.md)
- [Этап 4 — verification, release-evidence и закрытие](./33-4-integration-provider-sdk.md)

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- renderer/model никогда не получает raw secrets;
- provider manifest не может расширить Core security policy;
- required scopes валидируются перед effect;
- action risk metadata не может понизить обязательную Core policy;
- external endpoints фиксированы/allowlisted provider adapter, где возможно;
- redirect/response/output bounded;
- credentials не экспортируются в Workflow Package;
- secret values redacted из trace;
- provider code не устанавливается автоматически из импортированного workflow.

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

- [ ] Есть versioned provider/action contracts.
- [ ] Есть единый credential reference lifecycle.
- [ ] Actions содержат schemas/scopes/risk metadata.
- [ ] Workflow ссылается на stable provider/action identities.
- [ ] Есть dependency report при удалении credential.
- [ ] Webhook capability можно объявить отдельно от runtime.
- [ ] Built-in actions имеют test fixtures.
- [ ] Secrets остаются внутри Core credential boundary.

## Ограничения и non-goals

- публичный plugin marketplace;
- загрузка и выполнение произвольного provider-кода из сети;
- замена MCP;
- хранение OAuth/API secrets в workflow;
- выдача credentials модели;
- SaaS multi-tenant credential service;
- автоматическое предоставление scopes без пользовательского consent.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#13 Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures](https://github.com/rkfsociety/EvoHime/issues/13)
