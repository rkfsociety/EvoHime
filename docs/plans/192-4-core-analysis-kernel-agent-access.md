# План 192.4 — verification, release evidence и закрытие

Статус: active implementation contract. Этап следует после 192.1–192.3.

## Scope и изменяемые контракты

Подтвердить observed-поведение направления и перенести контракт из плана в
канонические документы.

1. Прогнать focused gates: contract/schema тесты `tool-runtime`, Core
   kernel-host и recovery тесты, IPC-проекцию, renderer-тесты вкладки.
2. Проверить, что в `core_agent_execution.rs` не появился дублирующий dispatch
   и что `memory.search`-подобная подмена не скопирована для `analysis.run`.
3. Проверить security-границы: нет файлового/сетевого/shell/credential доступа
   из kernel, нет значений объектов и inline result в UI, trace, логах и
   release evidence, нет обхода approval/receipt пути.
4. Проверить recovery на реальном сценарии: рестарт Core при активной
   kernel-сессии, отказ worker, превышение лимита, повтор с тем же
   idempotency key.
5. Обновить канонические документы: контракт — `docs/architecture.md`
   (раздел Persistent Analysis Kernel), состояние — `docs/current-state.md`,
   проверки — `docs/release-evidence.md`, каталог — `docs/plans/README.md`.
6. Удалить временные plan-файлы 192 после подтверждения критериев и переноса
   контракта.

## Зависимости

### Блокирующие

Завершённые 192.1–192.3 и свежие проверки затронутых поверхностей; правило
`AGENTS.md` о release-версиях затронутых публикуемых модулей и о task-only
commit.

### Опциональные

Windows acceptance на установленной сборке — желателен, но проверка
исправления на installed-версии требует публикации модуля и обновления,
поэтому не обязательна для закрытия контракта.

## Recovery и rollback

План остаётся активным, если хотя бы один критерий не подтверждён evidence.
Отключение направления — снятие объявления инструмента и UI-запроса; kernel
IPC-команды, storage и worker продолжают работать, история сессий не удаляется.

## Verification

Focused Rust tests для `tool-runtime` и `evohime-core` (kernel, IPC, recovery),
renderer-тесты вкладки, `npm run check:protocol`, `git diff --check`, проверка
внутренних ссылок. Полные сборки выполняет CI после push; локально — только
лёгкие проверки по правилу 4 `AGENTS.md`.

## Release evidence

Собрать: id/версии инструментов, canonical hash декларации, mapping операций,
версию kernel-контракта, tag IPC-команды, focused test/CI evidence, redacted
результаты recovery, подтверждение неизменности SQLite-схемы, перечень
поднятых release-маркеров. Не сохранять значения объектов, inline result,
raw prompts, secrets и абсолютные пути.

## Критерии выхода

- [ ] Все критерии 192.0 подтверждены evidence.
- [ ] Канонические документы обновлены, plan-файлы 192 удалены.
- [ ] Release-маркеры затронутых модулей подняты по правилу `AGENTS.md`.
- [ ] `git diff --check` и проверка ссылок проходят.
