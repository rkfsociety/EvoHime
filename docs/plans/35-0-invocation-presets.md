# План 35.0 — Invocation Presets: version-pinned шаблоны запусков без копирования секретов

Статус: предложено по [issue #15](https://github.com/rkfsociety/EvoHime/issues/15). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Invocation Presets**: сохранённые конфигурации запуска workflow, которые хранят значения inputs, выбор credential references и параметры запуска, но не копируют credential secrets.

Preset отвечает на вопрос:

> Как быстро повторить именно этот удачный запуск с теми же настройками?

Это отдельная сущность от workflow definition, schedule и TaskCheckpoint.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/invocation_presets.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./35-1-invocation-presets.md)
- [Этап 2 — runtime-интеграция и recovery](./35-2-invocation-presets.md)
- [Этап 3 — IPC, client projection и UI](./35-3-invocation-presets.md)
- [Этап 4 — verification, release-evidence и закрытие](./35-4-invocation-presets.md)

## Зависимости

### Блокирующие

- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 33.0 — Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures.
- План 30.0 — Workflow Package: export/import presets — только отдельная
  optional portable-форма; user-specific presets не экспортируются по умолчанию.
- План 34.0 — Event Trigger Runtime: optional base-preset mapping для trigger
  inputs с fail-closed degradation до обычного event mapping.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
InvocationPreset {
  id,
  owner_scope,
  name,
  description?,
  workflow_id,
  workflow_version,
  workflow_definition_hash,
  input_schema_hash,
  input_values,
  credential_bindings,
  execution_options,
  created_from_run_id?,
  revision,
  created_at,
  updated_at,
  content_hash
}
```

Ключевые инварианты: preset **прибит к конкретной workflow version и hash
снимка definition/input schema**, а каждая правка создаёт новую immutable
`revision`. `content_hash` считается по canonical redacted payload и не
включает secret material.

Новая версия workflow не должна молча менять смысл старого preset.

### Безопасность

- raw credential secrets не сохраняются;
- Secret inputs по умолчанию не persisted;
- preset не расширяет grants;
- preset не отключает approvals;
- credential scopes проверяются заново при каждом run;
- workflow version pinned;
- renderer получает masked values для sensitive fields;
- удаление или expiry credential переводит binding в `NeedsRebinding`, а не
  оставляет dangling secret cache;
- schedule фиксирует `preset revision` и `content_hash` либо эквивалентный
  resolved snapshot;
- trigger может использовать preset только как optional base configuration;
  event mapping не переопределяет credential/capability identities.

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

- [ ] `C01` — Есть durable InvocationPreset contract.
- [ ] `C02` — Preset pinned к workflow version.
- [ ] `C03` — Можно создать preset из completed run.
- [ ] `C04` — Credentials хранятся только как refs.
- [ ] `C05` — Secret inputs не сохраняются raw по умолчанию.
- [ ] `C06` — Есть migration flow между workflow versions.
- [ ] `C07` — Preset запускается через обычный workflow runtime.
- [ ] `C08` — Preset можно использовать scheduler без обхода approvals.
- [ ] `C09` — Preset можно создать вручную из workflow detail.
- [ ] `C10` — Удалённый/expired credential даёт `NeedsRebinding`.
- [ ] `C11` — Временный override не изменяет сохранённую revision.
- [ ] `C12` — Schedule фиксирует revision/hash snapshot.
- [ ] `C13` — Version drift показывает preview и не выполняет silent migration.
- [ ] `C14` — Trigger base mapping optional и не переопределяет protected identities.

## Ограничения и non-goals

- копирование workflow ради каждого набора inputs;
- хранение raw secrets;
- auto-migration через несовместимый schema change;
- standing approvals;
- публичный обмен user-specific presets;
- изменение workflow graph через preset;
- запуск без explicit user/schedule/trigger policy.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#15 Invocation Presets: version-pinned шаблоны запусков без копирования секретов](https://github.com/rkfsociety/EvoHime/issues/15)

## Результат ревью 2026-08-29

- Модель дополнена owner scope, immutable revision и hash снимков workflow
  definition/input schema; зафиксированы canonical redacted hash и schedule
  snapshot.
- Требования issue разложены по acceptance IDs: manual creation, completed-run
  sanitization, rebinding, drift/migration, temporary override, scheduler и
  optional trigger base configuration.
- План 30 переведён из blocking в optional: его portable export не нужен для
  локального durable preset и по умолчанию исключает user-specific presets.
