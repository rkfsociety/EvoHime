# План 204.4 — Верификация, полная зачистка тестов, документация и закрытие

## Scope
Финальная проверка, зачистка остаточных ссылок на старые имена, обновление канонической документации и релизных метаданных:
- поиск по всему репозиторию (`crates/`, `desktop/`, `docs/`) на отсутствие устаревших канонических имён (`filesystem_read`, `filesystem_write`, `filesystem_patch`, `filesystem_search`, `filesystem_list`, `shell_execute`, `http_fetch`, `process_run`) в коде, промптах, схемах, UI и тестах;
- обновление документации (`docs/architecture*.md`, `docs/current-state.md`, `crates/tool-runtime/README.md`, `docs/release-evidence.md`);
- обновление `release-versions/<module>.txt` и `release-notes/<module>.md` для всех затронутых публикуемых модулей (`core`, `cli`, `tools-exec`, `ui-bundle`, `shell-host` по `scripts/module-release.ps1`).

## Зависимости
- **Blocking:** [План 204.1](./204-1-core-agent-tool-naming-standardization.md), [План 204.2](./204-2-core-agent-tool-naming-standardization.md), [План 204.3](./204-3-core-agent-tool-naming-standardization.md).
- **Optional:** нет.

## Изменяемые контракты
- Документация контрактов и релизные манифесты затронутых модулей.

## Recovery и безопасность
- Подтверждение, что после удаления старых имён ни один путь восстановления или политики безопасности не теряет привязку к инструменту.

## Verification и Release Evidence
- Полный прогон CI: `core.yml`, `cli.yml`, `tools-exec.yml`, `ui-bundle.yml`, `shell-host.yml`, `windows.yml`, `Rust documentation` (`missing_docs`, `broken_intra_doc_links`, doctests).
- В `desktop/evohime-electron`: `npm run check:protocol`, `npm run typecheck`, `npm test`, `npm run build`, `npm run check:bundle`.
- Реальный прогон задачи через установленную Еву после публикации модулей: проверка, что модель вызывает `read`, `write`, `edit`, `grep`, `glob`, `bash` без отказов по имени инструмента.

## Rollback и закрытие
- После подтверждения всех проверок и переноса итогового контракта в каноническую документацию временные файлы `204-0`...`204-4` удаляются из `docs/plans/` согласно правилам каталога.
