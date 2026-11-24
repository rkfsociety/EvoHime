# План 200.4 — Verification, release evidence и закрытие

Статус: planned. Обзор: [`200-0-hive-queen-router-and-dynamic-agents.md`](200-0-hive-queen-router-and-dynamic-agents.md).

## Scope и критерии закрытия

1. Полная проверка маршрутизации Королевы без прямых инструментов, наследования per-task `session_id` мутирующими ролями, создания новых ролей/секторов и отображения движения машин в 3D-сцене.
2. Перенос утверждённого контракта в [`../architecture/hive.md`](../architecture/hive.md), обновление [`../current-state.md`](../current-state.md) и [`../release-evidence.md`](../release-evidence.md).
3. Удаление временных файлов `200-0` .. `200-4` из `docs/plans/` после закрытия по правилам [`README.md`](README.md).

## Зависимости

### Блокирующие
- Завершение [`200-1`](200-1-hive-queen-router-and-dynamic-agents.md), [`200-2`](200-2-hive-queen-router-and-dynamic-agents.md) и [`200-3`](200-3-hive-queen-router-and-dynamic-agents.md).

### Опциональные
- Отсутствуют.

## Recovery и rollback

- На этапе закрытия изменения кода уже стабилизированы; rollback выполняется на уровне конкретного модуля (`core`, `hive`, `ui-bundle`).

## Verification

- CI: `cargo test` для `evohime-core`, `evohime-hive`, `evohime-tool-runtime`, `rustdoc` gates, `npm run typecheck` и `npm test` в `desktop/evohime-electron`, `git diff --check`.

## Release evidence

- Зафиксированные результаты CI-прогонов, обновлённые `release-versions/` и `release-notes/` затронутых модулей.
