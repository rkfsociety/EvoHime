# План 191.0 — Core-owned tool surface hardening и доставка skills агенту

Статус: active implementation contract. Исходной issue нет: направление выведено из
аудита checkout, план не означает, что перечисленные дефекты уже исправлены.

## Цель

Привести объявленную поверхность встроенных инструментов в соответствие с
фактически исполняемой: одна декларация вместо duplicating-регистрации и
dispatch, честные input schemas, отсутствие рекламируемых, но неисполнимых
инструментов, детерминированный отбор на русских и английских запросах и
предсказуемая интеграция `git.commit` с правилом task-only. Затем довести
существующий Core-owned Skill Registry до модели: каталог и тело trusted-скилла
становятся доступны агенту через bounded progressive disclosure, а не только
через UI.

## Основание checkout

- `crates/tool-runtime/src/registry.rs:242-611` регистрирует 66 инструментов
  вручную; `registry_tests.rs:26` фиксирует это число.
- Dispatch продублирован дважды (`registry.rs:783-886` и `938-1036`), проверка
  policy — трижды (`674-701`, `737-780`, `919-936`).
- `manifest.rs:120-123` возвращает заглушку `{"input":{...}}` для всех
  инструментов без явной ветки (в том числе `cargo.*`, `archive.*`, `logs.*`,
  `git.status/log/show/blame`, `filesystem.stat`), при этом
  `additionalProperties:false` объявлен обязательным (`manifest.rs:264`).
- `manifest.rs:44-47` объявляет `filesystem.patch` без обязательного
  `expected_hash`, который требует `tools/patch.rs:22`.
- `manifest.rs:116` объявляет схему для незарегистрированного `process.spawn`.
- `tools/agent.rs:19-23` и `tools/browser_session.rs:48-50` всегда возвращают
  ошибку, но остаются в реестре, в каталоге модели и в
  `evohime-core/src/legacy_parser.rs:5-33`.
- `tools/git.rs:157` выполняет `git add -A` перед commit.
- `ipc_bridge_header.rs:37` объявляет `EXPECTED_TOOL_COUNT = 32` при 66
  фактических, поэтому сравнение `registered >= expected` в
  `doctor.rs:409-420` не сигналит о деградации.
- `adaptive_tool_catalog.rs:296-303` токенизирует только ASCII, а
  `191-201` при нулевом совпадении отдаёт алфавитно первые 8 инструментов, то
  есть `agent.run`, `app.*`, `archive.*`, `browser.*`.
- `adaptive_tool_catalog.rs:75-103` содержит `SelectionCache`, который не
  используется вне тестов.
- `core_prelude.rs:651-777` собирает системный промпт из ~25 код-условий без
  версии и профиля.
- `core_coordinator_tasks.rs:335` передаёт `skill_context: &[]`, поэтому
  существующий рендер секции (`workspace.rs:340`) никогда не наполняется, хотя
  `SkillRegistry` (`skill_registry.rs`), trust pipeline и IPC-команды
  `ListSkills`/`LoadSkill`/`LoadSkillReference` реализованы.

## Граница

`tool-runtime` остаётся владельцем декларации и исполнения инструментов;
`evohime-core` остаётся владельцем policy, отбора каталога, промпта, доставки
skills и durable trace. Направление не создаёт второй реестр инструментов,
второй skill registry, второй prompt owner или альтернативный approval-путь.

## Изменяемые контракты

- Декларативный builtin-каталог как единственный источник id, описания,
  permissions, timeout, input schema и исполняемости.
- Идентификатор версии системного промпт-профиля и его состав из метаданных
  каталога.
- Отбор инструментов: нормализация запроса, курируемый fallback-набор,
  переиспользование кэша проекции.
- Контракт `git.commit` (только staged-изменения либо явный список путей).
- Agent-facing поверхность skills: bounded discovery, загрузка тела по явному
  шагу, фиксация provenance в trace.

## Recovery и rollback

Неизменяемость сохраняется: право на вызов инструмента по-прежнему определяет
`PermissionEngine` и durable approval, а skill остаётся untrusted-инструкцией,
не расширяющей grant. Недоступный или невалидный skill не даёт доступа к телу;
неизвестный skill id — typed ошибка, не fallback на другой пакет. Отключение
нового промпт-профиля или новой схемы отбора возвращает прежнее поведение без
миграции данных и без удаления исторических trace-записей. Изменение
`git.commit` не должно оставлять частично применённое состояние: при отказе
индексации commit не выполняется.

## Этапы

- [Этап 1 — декларативный каталог, схемы и доступность инструментов](./191-1-core-tool-surface-and-skill-delivery.md)
- [Этап 2 — отбор инструментов, промпт-профиль и runtime-согласованность](./191-2-core-tool-surface-and-skill-delivery.md)
- [Этап 3 — доставка skills агенту и bounded projection](./191-3-core-tool-surface-and-skill-delivery.md)
- [Этап 4 — verification, release evidence и закрытие](./191-4-core-tool-surface-and-skill-delivery.md)

## Зависимости

### Блокирующие

- Существующие владельцы: `ToolRegistry`/`ToolManifest`, `PermissionEngine` и
  durable approval, `adaptive_tool_catalog`, `SkillRegistry` и
  `skill_trust_pipeline`, `EventJournal`.
- Закрытый план 38 (Adaptive Tool Catalog v1) и закрытый план 138 (Skill Source
  & Update Lifecycle): их контракт не переопределяется.

### Опциональные

- План 184 может использовать provenance загруженных skills как дополнительный
  вход trace evaluation; его отсутствие не блокирует этапы 1–3.
- План 186 не нужен: отбор остаётся детерминированным, semantic-селектор
  подключается только при наличии отдельного evidence.

## Критерии готовности

- [ ] Ни один инструмент не объявляется модели, если его dispatch гарантированно
      возвращает ошибку.
- [ ] Каждый зарегистрированный инструмент имеет явную input schema, валидную по
      `ToolManifest::validate`, и объявленные поля совпадают с полями, которые
      разбирает адаптер.
- [ ] Регистрация и dispatch читают одну декларацию; третьей копии `match` и
      повторной policy-проверки нет.
- [ ] Число ожидаемых инструментов выводится из каталога, и Doctor сигналит при
      фактическом расхождении.
- [ ] Отбор на типичном русском запросе возвращает релевантный набор, а не
      алфавитный префикс.
- [ ] `git.commit` не захватывает посторонние изменения рабочего дерева.
- [ ] Агент получает метаданные trusted-скиллов и загружает тело только явным
      шагом с bounded размером и grant-пересечением; trace фиксирует skill id,
      version и hash без тела.
- [ ] Static и focused test evidence приложены, canonical docs обновлены только
      после подтверждённого поведения.

## Non-goals

- Переписывание sandbox, revision-safe boundary, SSRF-защиты или
  approval/receipt-механизма.
- Реализация `agent.run` как настоящего subagent backend и восстановление
  `browser.session.*`.
- Семантический (model-based) выбор инструментов как обязательный путь.
- Исполнение skill-скриптов, install/network hooks или model invocation из
  skill package.
- Изменение release-версий модулей в рамках этого плана сверх правила
  `AGENTS.md` о затронутых публикуемых модулях.

## Verification

Плановые gates: contract/schema tests и проверка соответствия схем адаптерам →
отбор и промпт-профиль на фиксированных RU/EN запросах → интеграция skills с
grant/bounds/recovery → static verification, focused tests и Windows acceptance.
Проверки выполняются при реализации соответствующих этапов и не выдаются за
выполненные заранее.

## Release evidence

Хранить contract versions, список объявленных и фактически зарегистрированных
инструментов, результат проверки схем, профиль промпта и его hash, bounded
диагностику отбора и skills, focused test/CI evidence. Не сохранять raw prompts,
тела скиллов, secrets и абсолютные пути.

## Критерии выхода overview

- [ ] Ownership, backward compatibility и failure semantics согласованы до
      этапа 1.
- [ ] Каждый этап проверяет observed-поведение; ни один не создаёт второй
      реестр, второй skill owner или альтернативную policy-проверку.
