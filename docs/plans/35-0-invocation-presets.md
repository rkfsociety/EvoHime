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

Кандидатная точка интеграции: `crates/evohime-core/src/invocation-presets.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 30.0 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 33.0 — Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures.
- План 36.0 — Agent Benchmark Matrix: многократные model/strategy evals и regression tracking.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
InvocationPreset {
  id,
  name,
  description?,
  workflow_id,
  workflow_version,
  input_values,
  credential_bindings,
  execution_options,
  created_from_run_id?,
  created_at,
  updated_at,
  content_hash
}
```

Ключевой инвариант: preset **прибит к конкретной workflow version**.

Новая версия workflow не должна молча менять смысл старого preset.

### Безопасность

- raw credential secrets не сохраняются;
- Secret inputs по умолчанию не persisted;
- preset не расширяет grants;
- preset не отключает approvals;
- credential scopes проверяются заново при каждом run;
- workflow version pinned;
- renderer получает masked values для sensitive fields;
- удаление credential инвалидирует binding, а не оставляет dangling secret cache.

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

- [ ] Есть durable InvocationPreset contract.
- [ ] Preset pinned к workflow version.
- [ ] Можно создать preset из completed run.
- [ ] Credentials хранятся только как refs.
- [ ] Secret inputs не сохраняются raw по умолчанию.
- [ ] Есть migration flow между workflow versions.
- [ ] Preset запускается через обычный workflow runtime.
- [ ] Preset можно использовать scheduler без обхода approvals.

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
