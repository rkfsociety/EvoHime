# План 76.0 — Safe UI Extension Framework: declarative pages, panels и themes без renderer authority

Статус: предложено по [issue #56](https://github.com/rkfsociety/EvoHime/issues/56). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime безопасный **UI Extension Framework**: расширяемую систему, позволяющую устанавливаемым пакетам добавлять новые presentation surfaces в desktop-приложение, не превращая расширение в unrestricted код с полномочиями renderer/Core.

Это отдельная категория расширения продукта:

```text
Skill          -> меняет способы работы агента
Integration    -> подключает внешний сервис
Workflow       -> описывает исполняемый процесс
UI Extension   -> добавляет представление/интеракцию в приложение
```

Главный принцип:

> UI-расширение может расширять интерфейс, но не получает полномочия Core только потому, что умеет что-то нарисовать.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/safe-ui-extension-framework.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./76-1-safe-ui-extension-framework.md)
- [Этап 2 — runtime-интеграция и recovery](./76-2-safe-ui-extension-framework.md)
- [Этап 3 — IPC, client projection и UI](./76-3-safe-ui-extension-framework.md)
- [Этап 4 — verification, release-evidence и закрытие](./76-4-safe-ui-extension-framework.md)

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 47.0 — Skill Trust Pipeline: deterministic scanning, contextual review и quarantine перед активацией.
- План 56.0 — Artifact Handoff Registry: typed deliverables, lineage и freshness для multi-agent работы.
- План 66.0 — Typed Agent Handoff Contract: explicit transfer of task ownership and context.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Extension Package
  -> manifest + declarative contributions
  -> Core validation / trust / installation
  -> safe UI projection
  -> desktop host renderer
```

В первом этапе предпочтительно **не исполнять arbitrary extension JavaScript/Native DLL в renderer process**.

Основная модель v1: data-driven/declarative contributions, которые визуализируются собственными компонентами EvoHime.

Executable extension runtime можно рассматривать позже только через отдельный sandbox/process boundary и versioned narrow host bridge.

### Безопасность

- install != enable;
- manifest declarative в v1;
- no arbitrary same-process executable code by default;
- contribution ID обязан быть объявлен в manifest;
- data sources/actions только из Core registry;
- extension не расширяет capability grants;
- secrets не проецируются только ради UI;
- source/ref resolve в exact revision/hash;
- update с permission/capability delta требует review;
- extension scope изолирован между workspace/backend identities;
- path traversal внутри package запрещён;
- package size/file-count bounded.

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

- [ ] Есть versioned UiExtensionManifest.
- [ ] Installation и enablement разделены.
- [ ] V1 contributions declarative/host-rendered, без unrestricted renderer code.
- [ ] Data/actions привязаны к Core-owned registries.
- [ ] Revision/compatibility/trust фиксируются Core-side.
- [ ] Update permission delta видим и revalidated.
- [ ] Extension lifecycle scoped и recoverable.
- [ ] Ошибка extension не ломает основной UI.

## Ограничения и non-goals

- marketplace/монетизация;
- arbitrary native DLL/JavaScript внутри основного процесса;
- iframe/WebView как магическая гарантия безопасности;
- выдача extension прямого доступа к Core DB;
- direct shell/filesystem/network APIs для UI пакетов;
- automatic enablement скачанного кода;
- обязательная поддержка всех будущих contribution kinds сразу.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#56 Safe UI Extension Framework: declarative pages, panels и themes без renderer authority](https://github.com/rkfsociety/EvoHime/issues/56)
