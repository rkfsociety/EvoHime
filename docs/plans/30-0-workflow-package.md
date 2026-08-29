# План 30.0 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей

Статус: предложено по [issue #10](https://github.com/rkfsociety/EvoHime/issues/10). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime versioned **Workflow Package**: переносимый формат экспорта и импорта workflow, который можно сохранить в файл, перенести между машинами/workspaces, положить в Git или передать другому пользователю без утечки credentials, локальных идентификаторов и runtime state.

Формат должен переносить **определение автоматизации**, а не снимок конкретной установки.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/workflow_package.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./30-1-workflow-package.md)
- [Этап 2 — runtime-интеграция и recovery](./30-2-workflow-package.md)
- [Этап 3 — IPC, client projection и UI](./30-3-workflow-package.md)
- [Этап 4 — verification, release-evidence и закрытие](./30-4-workflow-package.md)

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

Ключевые инварианты:

- импорт package не выдаёт capabilities;
- definition не исполняется во время parse/import;
- credential secrets никогда не входят в экспорт;
- imported identity fields проходят текущую workflow registry validation;
- неизвестные tools/providers не создаются автоматически;
- package не может зарегистрировать новый executable tool;
- paths canonicalized, traversal запрещён;
- import не создаёт trigger/schedule без отдельного действия;
- secret/sensitive raw fields удаляются по authoritative metadata.

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

- [ ] Есть versioned package format.
- [ ] Export удаляет credentials/secrets/runtime-specific state.
- [ ] Есть dependency manifest.
- [ ] Import выполняет validate/resolve/preview до записи.
- [ ] Credential slots требуют локального rebinding.
- [ ] Сохраняется безопасная provenance/fork lineage.
- [ ] Canonical hash позволяет duplicate/diff detection.
- [ ] Import не расширяет Core capability registry.

## Ограничения и non-goals

- публичный marketplace;
- загрузка пакетов из интернета;
- автоматическая установка неизвестных tools/plugins;
- перенос активного run/checkpoint как будто это workflow definition;
- перенос секретов;
- executable package hooks;
- автоматическое включение schedules/triggers после импорта.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#10 Workflow Package: переносимый import/export без секретов и с rebinding зависимостей](https://github.com/rkfsociety/EvoHime/issues/10)
