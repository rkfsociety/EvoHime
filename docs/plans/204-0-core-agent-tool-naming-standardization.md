# План 204.0 — Стандартизация имён инструментов под привычный агентский лексикон

Статус: active implementation contract. Зависит от закрытого контракта канонического лексикона `196` (`^[a-z][a-z0-9_]*$`) и продолжает hardening поверхности инструментов из [Плана 191](./191-0-core-tool-surface-and-skill-delivery.md).

## Контекст и проблема
В `crates/tool-runtime`, `crates/evohime-core` и `desktop/evohime-electron` базовые инструменты имеют длинные внутренние имена (`filesystem_read`, `filesystem_write`, `filesystem_patch`, `filesystem_search`, `filesystem_list`, `shell_execute`, `http_fetch`) и дублирующиеся способы запуска команд (`shell_execute`, `process_run`, `script_execute`).

На практике (подтверждено трассировками работы агента и индустриальными практиками Claude Code / Codex / OpenCode / Anthropic & OpenAI tool use):
- модели регулярно вызывают по привычке `read_file` / `write_file` / `read` / `edit` / `grep` / `glob` / `bash` и получают отказ инструмента;
- длинные префиксы `filesystem_*` и пересечение `shell_execute` / `process_run` / `script_execute` создают synonym overload и лишние ошибки маршрутизации;
- `filesystem_list` ограничен листингом одного каталога, тогда как агентам нужен полноценный `glob` (поиск файлов по маске пути, например `**/*.rs`).

## Цель
Переименовать пользовательские/агентские инструменты на стандартный, привычный LLM-агентам набор без сохранения старых названий (`filesystem_*`, `shell_execute`, `http_fetch`) в канонической поверхности, сохранив строгую валидацию безопасности, права доступа и трассировку.

## Карта переименования

| Старое имя | Новое каноническое имя | Примечание |
| --- | --- | --- |
| `filesystem_read` | `read` | Чтение файла |
| `filesystem_write` | `write` | Создание и полная перезапись файла |
| `filesystem_patch` | `edit` | Точечная замена фрагмента в файле |
| `filesystem_search` | `grep` | Поиск по содержимому файлов |
| `filesystem_list` | `glob` | Поиск файлов по glob-маске + листинг каталога |
| `shell_execute` | `bash` | Основной инструмент выполнения команд |
| `http_fetch` | `web_fetch` | Загрузка веб-страниц по URL |
| `filesystem_delete` | `file_delete` | Удаление файла/каталога |
| `filesystem_move` | `file_move` | Перемещение/переименование |
| `filesystem_copy` | `file_copy` | Копирование |
| `filesystem_stat` | `file_stat` | Метаданные файла |
| `filesystem_mkdir` | `mkdir` | Создание каталога |

Доменные инструменты с уже устоявшимися именами (`git_*`, `cargo_*`, `memory_search`, `goal_*`, `workflow_*`, `browser_*`, `archive_*`, `logs_*`, `mcp_call`, `agent_run`) сохраняют текущие названия.
Пересечение `bash` / `process_run` / `script_execute` устраняется в рамках Этапа 1 (единый основной `bash` без двусмысленности).

## Ограничения и инварианты
- Все новые имена строго соответствуют каноническому лексикону `^[a-z][a-z0-9_]*$`.
- Устаревшие имена (`filesystem_*`, `shell_execute`, `http_fetch`) удаляются из канонических констант, JSON-схем, системных промптов, каталога инструментов, UI и тестов без остаточных ссылок.
- Границы песочницы (`WorkspaceSandbox`), проверка ревизий/хэшей файлов перед записью (`edit`/`write`), лимиты вывода и политика приватности (`sensitive_data_guardrails`) сохраняются полностью.

## Этапы реализации
- [Этап 1 — `tool-runtime`: константы, JSON-схемы, `glob` и консолидация `bash`](./204-1-core-agent-tool-naming-standardization.md)
- [Этап 2 — `evohime-core` и смежные Rust-крейты: каталог, права, квитанции, recovery и промпты](./204-2-core-agent-tool-naming-standardization.md)
- [Этап 3 — `evohime-electron` и `evohime-cli`: UI-проекции, карточки активности, переводы и CLI](./204-3-core-agent-tool-naming-standardization.md)
- [Этап 4 — Верификация, полная зачистка тестов, документация и релизные версии](./204-4-core-agent-tool-naming-standardization.md)

## Зависимости
- **Blocking:** нет (базовый контракт `196` уже закрыт).
- **Optional:** [План 191](./191-0-core-tool-surface-and-skill-delivery.md) (учитывает обновлённые имена в delivery/skill context).

## Критерии готовности
- [ ] Все этапы `1–4` выполнены и подтверждены тестами.
- [ ] В кодовой базе, схемах, промптах, UI и тестах не осталось старых канонических имён (`filesystem_*`, `shell_execute`, `http_fetch`).
- [ ] CI всех затронутых модулей (`core`, `cli`, `tools-exec`, `ui-bundle` и `Rust documentation`) зелёный.
