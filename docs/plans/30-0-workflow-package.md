# План 30.0 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей

Статус: предложено по [issue #10](https://github.com/rkfsociety/EvoHime/issues/10). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

Результат ревью 2026-08-29: контракт уточнён по issue и live `workflow/v1`;
package import/export отделён от runtime, добавлены portable envelope,
resolution/preview/commit и atomic recovery gates. Обоснованных замечаний к
границе направления и порядку этапов не осталось.

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
не повторять side effect вслепую. Package file read/write выполняется только
Core-owned bounded file boundary: canonicalized path, allowed extension/size,
atomic temp-to-final write и отсутствие произвольного archive extraction.

Базой является уже реализованный контракт `workflow/v1` в
`crates/evohime-core/src/workflow.rs`, его `WorkflowRegistry` и существующие
immutable workflow definitions. Новый контур не создаёт второй графовый
контракт и не заменяет `workflow_runtime.rs`.

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

- Существующий `workflow/v1`, `WorkflowRegistry` и canonical JSON/hash: пакет
  сериализует уже разрешённое определение, но не получает права расширять его.
- Существующие Core-owned capability/policy/approval, event journal, SQLite
  transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 31.0 может открыть импортированное определение в builder, но package
  contract и import/export должны работать без builder; это не dependency.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Канонический package contract

Envelope получает отдельную версию формата, не совпадающую с `workflow/v1`:
`format = "evohime-workflow"`, `format_version`, stable logical
`workflow_id`/`workflow_version`, name/description, graph, input/output schema,
dependency manifest, required capabilities, credential slots, context
requirements, optional recommended schedule, provenance, creation metadata и
`content_hash`. Локальные PK, run/checkpoint/lease/approval/session IDs и
machine-specific state в envelope не входят. `content_hash` вычисляется от
portable canonical content, а не от времени создания, layout или локальных
идентификаторов.

Dependency entry содержит `kind`, `logical_id`, optional required version and
schema hash, `optional` и bounded notes. Credential slot содержит только
логический id, provider/auth kind, required scopes и users of the slot;
rebind привязывает его к локальному Core-owned reference, а не экспортирует
значение секрета.

Первая реализация — bounded JSON (либо JSON с allow-listed non-executable
assets); архив, executable hooks и произвольные assets не входят в scope.

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

1. Зафиксировать versioned typed contract, import state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core export и import preview/commit поверх существующего
   workflow contract. Durable import history/version mapping, если она нужна,
   должна быть additive и транзакционной; deterministic serialization/hash
   обязателен для portable content.
3. Подключить существующие registry/tool/workflow/provider/child контуры для
   dependency resolution и повторной grant/policy/approval проверки; package
   import не добавляет собственных retry/cancellation semantics.
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
- [ ] Import до explicit commit не пишет workflow/version, не создаёт
  schedule/trigger и не запускает graph.

## Ограничения и non-goals

- публичный marketplace;
- загрузка пакетов из интернета;
- автоматическая установка неизвестных tools/plugins;
- перенос активного run/checkpoint как будто это workflow definition;
- перенос секретов;
- executable package hooks;
- автоматическое включение schedules/triggers после импорта.
- runtime orchestration, lease/retry и восстановление активного workflow run;
  пакет импортирует definition, а не execution.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#10 Workflow Package: переносимый import/export без секретов и с rebinding зависимостей](https://github.com/rkfsociety/EvoHime/issues/10)
