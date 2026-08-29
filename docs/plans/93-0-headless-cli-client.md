# План 93.0 — Headless CLI Client: NDJSON streaming, one-shot runs и automation поверх существующего Core

Статус: предложено по [issue #73](https://github.com/rkfsociety/EvoHime/issues/73). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime официальный **headless CLI client**, который использует тот же локальный EvoHime Core и те же typed contracts, что desktop UI, но позволяет запускать, наблюдать и продолжать agent tasks из terminal, scripts и CI без создания второго agent runtime.

Главный принцип:

```text
Desktop UI ─┐
            ├─ typed IPC/RPC -> EvoHime Core -> agent/workflow/tool runtime
CLI Client ─┘
```

CLI является ещё одной presentation/control surface. Он не дублирует Core, database, permissions, approvals или agent loop.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/headless-cli-client.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./93-1-headless-cli-client.md)
- [Этап 2 — runtime-интеграция и recovery](./93-2-headless-cli-client.md)
- [Этап 3 — IPC, client projection и UI](./93-3-headless-cli-client.md)
- [Этап 4 — verification, release-evidence и закрытие](./93-4-headless-cli-client.md)

## Зависимости

### Блокирующие

- План 57.0 — Plan Artifact: versioned planning contract и явный переход Plan → Execute.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 77.0 — Headless Core CLI: non-interactive agent/workflow runs для CI, scripts и NDJSON automation.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- CLI всегда идёт через Core;
- local IPC защищён OS-user ACL;
- no API keys/secrets in argv;
- CLI не имеет direct shell/filesystem authority поверх Core;
- workspace paths canonicalized;
- non-interactive mode не auto-approves;
- profile/model ids resolve through Core registry;
- event/output projection проходит sensitive-data policy;
- artifact export имеет path/overwrite/sensitivity checks;
- client cannot spoof run/agent identity.

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

- [ ] Есть официальный CLI, использующий существующий EvoHime Core.
- [ ] Есть human и versioned NDJSON output modes.
- [ ] One-shot, detach, resume/follow и cancel работают через typed commands.
- [ ] CLI использует Conversation Event Log cursor/reconnect semantics.
- [ ] Non-interactive mode fail-closed при missing approval.
- [ ] Stable exit codes/documented machine outcomes.
- [ ] Workspace/profile/context refs resolve через Core registries.
- [ ] CLI не создаёт второй agent/security runtime.

## Ограничения и non-goals

- второй embedded Core внутри CLI;
- remote public daemon API;
- передача API keys в argv;
- `--yolo`/approve-everything режим;
- полный terminal UI clone desktop приложения;
- собственная CLI database/history;
- обход desktop/Core policies ради CI.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#73 Headless CLI Client: NDJSON streaming, one-shot runs и automation поверх существующего Core](https://github.com/rkfsociety/EvoHime/issues/73)
