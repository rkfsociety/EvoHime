# План развития Евы по мотивам agency-agents

## Основание и вывод

Изучен репозиторий [msitarzewski/agency-agents](https://github.com/msitarzewski/agency-agents), зафиксированный на commit `ebe9c99acb5c96f9468de368d8bead775387d1a7`.

Это не готовый runtime для EvoHime, а каталог специализированных ролей: агент описывается markdown-файлом с YAML frontmatter, миссией, рабочим процессом, deliverables, метриками успеха и стилем общения. Репозиторий группирует роли по дивизионам, поддерживает playbook/runbook-оркестрацию, handoff-шаблоны, Dev↔QA-цикл, lint/originality-проверки и конвертацию в несколько форматов инструментов. `divisions.json` и `tools.json` выступают источниками истины для каталога и интеграций.

Для Евы полезно заимствовать модель «роль + контракт результата + доказательства», но не переносить весь внешний roster и его установщик. EvoHime уже имеет native WinUI 3/C# UI, Rust Core, supervisor, SQLite, versioned named-pipe IPC, `agent.run`, risk/approval-контур, event journal и собственную trust-модель плагинов. Все новые роли должны исполняться Core, а UI только выбирать роль и показывать состояние.

## Цель

Создать в Еве управляемый каталог локальных специализированных профилей, которые:

- помогают выбирать правильный режим работы и набор проверок;
- дают предсказуемый формат результата, handoff и acceptance criteria;
- могут безопасно объединяться в ограниченные workflow;
- сохраняют trace и измеримые evidence вместо голословного «готово»;
- не получают новых прав автоматически и не обходят существующие approval, sandbox, IPC и plugin-gate.

## Что не переносить

1. Не копировать сотни чужих ролей целиком. Сначала нужен небольшой EvoHime-native roster, отражающий Rust/WinUI/Windows/IPC/SQLite и правила этого репозитория.
2. Не добавлять внешний multi-tool installer, запись в `~/.codex`, `.claude` и аналогичные каталоги: Ева не должна превращаться в прокси для сторонних агентских конфигураций.
3. Не превращать `personality`, emoji или marketing-описание в разрешение на инструмент. Возможности определяются Core policy, а не frontmatter.
4. Не запускать параллельных дочерних агентов без budget, cancellation, parent/child trace и отдельного approval-гейта.
5. Не импортировать тексты и примеры без проверки лицензии, происхождения, prompt-injection и соответствия русской локализации. Для встроенных ролей вести attribution/license metadata.

## Целевая модель

### 1. Роль как версионируемый профиль

Добавить source-of-truth каталог ролей, например `resources/agent-roles/`, а загрузку и валидацию оставить в Rust Core. Каждый профиль содержит:

- `id`, `version`, `name`, `description`, `division`;
- `mission` и допустимые типы задач;
- `system_instructions` или ссылку на локализованный body;
- `allowed_tools`, `default_risk`, `requires_approval`;
- обязательный входной контекст и формат deliverables;
- acceptance criteria и требуемые evidence;
- `handoff_targets`, лимиты retry/time/token и `supports_read_only`;
- `source`, `license`, checksum и дату ревизии.

Frontmatter удобен авторам, но перед использованием должен преобразовываться в строгую схему и проверяться Core. Невалидный или неизвестный permission profile не активируется.

### 2. Дивизионы EvoHime

Начальный набор не более 10–12 ролей:

| Дивизион | Первая роль | Основной результат |
| --- | --- | --- |
| Core Engineering | Rust Core Engineer | изменение crate с тестами и границами ответственности |
| Desktop | WinUI Shell Engineer | native UI/state/IPC-изменение без бизнес-логики в UI |
| IPC & Reliability | IPC Compatibility Engineer | proto/envelope/replay/cancellation и compatibility evidence |
| Storage | SQLite Migration Engineer | транзакционная миграция, backup/rollback и тесты |
| Security | Threat Model Reviewer | threat findings, risk classification и remediation plan |
| Testing | Evidence Collector | воспроизводимые команды, артефакты и PASS/FAIL |
| Testing | Reality Checker | проверка фактического результата против acceptance criteria |
| Delivery | Windows Packaging Engineer | package/installer/update/recovery verification |
| Documentation | Technical Writer | актуализация docs/current-state/roadmap и эксплуатационных инструкций |
| Coordination | Task Orchestrator | декомпозиция, handoff, retry budget и финальный сводный отчёт |

Позже можно добавлять роли для browser, provider, memory, plugins и пользовательских доменов как отдельные signed packs, а не смешивать их с базовыми системными ролями.

### 3. Роль не равна отдельной личности

Профиль роли задаёт рабочий контракт. Имя «Ева», русский язык, женский род и правила обращения остаются глобальными инструкциями Core. Роль может менять специализацию и формат отчёта, но не может отменять проектные правила, security policy, approval или delivery-gate.

## Этапы реализации

### Этап 0. Контракт и политика каталога

1. Зафиксировать JSON Schema профиля роли и отдельную схему workflow/handoff.
2. Определить приоритеты инструкций: проектные правила → Core policy → роль → запрос пользователя → данные workspace.
3. Разделить capability (`read`, `write`, `shell`, `git`, `browser`, `network`, `memory`) и risk/approval. Поле роли не может расширять глобальный allowlist.
4. Добавить checksum, version, source/license и обязательную redaction policy.
5. Описать lifecycle: bundled, installed, enabled, quarantined, disabled, removed.

Проверка: schema/lint отвергает отсутствующие обязательные поля, неизвестные инструменты, write-capability у read-only роли и невалидные версии.

### Этап 1. Встроенный каталог и Core registry

1. Добавить первые EvoHime-native профили из таблицы выше, без копирования внешних текстов.
2. Реализовать `AgentRoleRegistry` в Rust Core: list/get/validate/resolve profile.
3. Хранить активный profile/version в task metadata и event journal.
4. Добавить deterministic prompt assembly: role contract + project context + task context + tool policy.
5. Сделать миграцию SQLite только после тестов и backup-path; UI не читает файлы профилей напрямую.

Проверка: два запуска с одной ролью получают один и тот же assembled contract; недоступный профиль даёт понятное событие `role.unavailable`; trace не содержит секретов.

### Этап 2. Выбор роли в native UI и IPC

1. Добавить Core-owned запрос каталога ролей и IPC-команду выбора профиля.
2. В WinUI показать division, назначение, risk, read-only/approval-состояние и версию.
3. Добавить безопасные presets: `Исследование`, `Реализация`, `Проверка`, `Релизная проверка`.
4. Оставить ручной выбор конкретной роли опциональным: по умолчанию Core выбирает preset/role по задаче и показывает объяснение.
5. При reconnect/replay восстанавливать выбранную роль из task state, а не из локального UI-состояния.

Проверка: UI не может активировать disabled/quarantined profile и не может сам изменить tool policy; старые IPC-клиенты продолжают работать с default profile.

### Этап 3. Handoff и workflow-оркестрация

1. Ввести Core-сущность `RoleRun` и связи `parent_run_id`/`child_run_id`.
2. Реализовать строгий handoff-документ: задача, состояние, файлы, зависимости, deliverables, acceptance criteria, evidence и ограничения.
3. Разрешить только объявленные workflow, например:
   - `Исследование → План → Реализация → Evidence Collector → Reality Checker`;
   - `Изменение IPC → Compatibility Review → Rust/C# tests → Packaging smoke`;
   - `Ошибка релиза → Diagnostics → Recovery plan → Verification`.
4. Для дочерних read-only runs запретить write/shell/commit независимо от текста prompt.
5. Добавить cancellation propagation, timeout, максимум три retry и обязательную эскалацию после исчерпания retry budget.
6. Писать parent/child, handoff и gate events в SQLite/event journal.

Проверка: workflow нельзя продвинуть без заполненного handoff и evidence; остановка родительской задачи останавливает дочерние runs; повторный запуск не дублирует side effect.

### Этап 4. Evidence-first QA gates

1. Перенести полезную идею `Evidence Collector` и `Reality Checker` в native-пайплайн.
2. Для каждого workflow хранить acceptance criteria как проверяемые пункты, а не только текстовую инструкцию.
3. Поддержать evidence types: command result, test result, diff summary, screenshot/package artifact, IPC replay proof, redacted log excerpt.
4. Разделить verdicts `PASS`, `FAIL`, `NEEDS_WORK`, `BLOCKED`; запретить автоматическое объявление `READY` без требуемых доказательств.
5. Показывать в UI источник каждого verdict и незакрытые критерии.
6. Добавить метрики: first-pass rate, retry count, blocked reason, duration, tool/risk usage и false-ready incidents.

Проверка: синтетический workflow с отсутствующим доказательством остаётся `NEEDS_WORK`; evidence доступно в replay после перезапуска Core; чувствительные данные редактируются до сохранения.

### Этап 5. Авторинг, lint и локализация

1. Добавить Rust/PowerShell-проверку профилей: schema, duplicate id, missing division, invalid tool, broken handoff target, prompt length и запрещённые claims.
2. Проверять оригинальность и дублирование только как качество каталога, не как security gate.
3. Ввести русскую canonical-версию встроенных ролей; английские термины допустимы только в идентификаторах/API.
4. Добавить fixtures для каждого профиля и snapshot assembled prompt с redacted placeholders.
5. Обновить `docs/plugin-management-7.8.md`: role packs проходят тот же manifest/hash/quarantine/approval-контур, что и плагины.

Проверка: CI проверяет все bundled profiles; package содержит только прошедшие lint роли; profile body не может содержать секреты или необработанные внешние инструкции.

### Этап 6. Signed role packs и каталог расширений

1. После стабилизации bundled roster расширить plugin manifest для `role-pack`.
2. Поддержать install/update/disable/quarantine/uninstall с lock-файлом, hash, signature и rollback.
3. Разрешения пакета должны быть явными и показываться до установки; новая роль не получает доступ к workspace автоматически.
4. Добавить локальный каталог установленных пакетов и provenance в UI.
5. Не делать удалённый marketplace обязательной частью MVP; сначала нужны локальный файл/репозиторий и ручной approval.

Проверка: повреждённый hash, недействительная подпись, несовместимый schema version и просроченный пакет не активируются; uninstall обратим и оставляет audit trail.

## Предлагаемая раскладка файлов

```text
resources/agent-roles/
  manifest.json
  engineering/rust-core-engineer.md
  desktop/winui-shell-engineer.md
  reliability/ipc-compatibility-engineer.md
  testing/evidence-collector.md
  testing/reality-checker.md
crates/evohime-core/src/agent_roles.rs
crates/evohime-core/tests/agent_roles.rs
crates/evohime-local-storage/src/lib.rs       # migrations/metadata/events
crates/desktop-ipc/proto/evohime.desktop.proto
desktop/EvoHime.Desktop/Services/AgentRoleCatalogService.cs
docs/agent-roles.md
docs/workflows/role-workflows.md
scripts/agent-role.tests.ps1
```

Пути уточнить перед реализацией с учётом текущего packaging manifest. UI-сервис остаётся адаптером IPC, а не владельцем каталога.

## Порядок поставки

1. Этап 0 и два read-only профиля (`Evidence Collector`, `Reality Checker`).
2. Core registry и task metadata без пользовательских role packs.
3. Native UI catalog + выбор preset + compatibility tests.
4. `RoleRun`, handoff, bounded workflow и cancellation.
5. Evidence gates и metrics.
6. Остальные системные роли и русская документация.
7. Только после стабильности — signed role packs и расширение plugin catalog.

Каждый этап реализуется Евой через штатный Core с конечным timeout и trace попытки. После каждой успешной задачи обязательны focused Rust tests, WinUI/IPC tests, `git diff --check`, native workflow/package smoke, очистка артефактов и отдельный task-only commit. Новые IPC-сообщения всегда обновляются на Rust и C# сторонах одновременно.

## Критерии готовности

- Ева умеет выбрать профиль и показать его назначение, версию, риск и требуемые approval.
- Core неизменно применяет policy независимо от текста роли и UI.
- Простой workflow передаёт полный handoff между ролями и сохраняет parent/child trace.
- Ни один `PASS`/`READY` не появляется без фактических evidence.
- Read-only профили не могут писать файлы, запускать shell или создавать commit.
- Роли, trace, handoff и verdict восстанавливаются после перезапуска Core.
- Повреждённые и неподписанные role packs не активируются.
- Native UI остаётся thin client, а существующие IPC replay/cancellation/approval и plugin trust guarantees не ухудшаются.

## Источники

- [README агентского каталога](https://github.com/msitarzewski/agency-agents/blob/ebe9c99acb5c96f9468de368d8bead775387d1a/README.md) — модель специализированных ролей, дивизионы, workflow и multi-tool integration.
- [divisions.json](https://github.com/msitarzewski/agency-agents/blob/ebe9c99acb5c96f9468de368d8bead775387d1a/divisions.json) — источник истины каталога дивизионов.
- [tools.json](https://github.com/msitarzewski/agency-agents/blob/ebe9c99acb5c96f9468de368d8bead775387d1a/tools.json) — разделение формата, scope и install kind.
- [пример профиля Code Reviewer](https://github.com/msitarzewski/agency-agents/blob/ebe9c99acb5c96f9468de368d8bead775387d1a/engineering/engineering-code-reviewer.md) — структура frontmatter, mission, checklist и communication style.
- [agent activation prompts](https://github.com/msitarzewski/agency-agents/blob/ebe9c99acb5c96f9468de368d8bead775387d1a/strategy/coordination/agent-activation-prompts.md) и [handoff templates](https://github.com/msitarzewski/agency-agents/blob/ebe9c99acb5c96f9468de368d8bead775387d1a/strategy/coordination/handoff-templates.md) — идеи NEXUS-оркестрации и передачи контекста.
- [CONTRIBUTING.md](https://github.com/msitarzewski/agency-agents/blob/ebe9c99acb5c96f9468de368d8bead775387d1a/CONTRIBUTING.md) — lint, originality, metadata, compatibility и правила расширения каталога.
