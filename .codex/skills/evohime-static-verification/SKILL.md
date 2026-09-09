---
name: evohime-static-verification
description: Выполнить финальную разрешённую статическую проверку EvoHime при запрете локального тестирования, сборок, линтеров и smoke-прогонов.
---

# Статическая проверка без локального запуска

## Запрещено

Не запускай `cargo test`, `cargo check`, `cargo build`, `cargo clippy`,
`cargo fmt`, `npm test`, `npm run typecheck`, `npm run build`, package scripts,
E2E, smoke scripts, `start-dev.ps1`, `test-agent.ps1`, native packaging и
любой runtime. Не запускай «быстрый» или узкий вариант вместо полного:
пользовательский запрет распространяется на все локальные tests/builds/linters
и иные исполняемые проверки.

## Разрешено

Только чтение и статический анализ: `git status`, `git diff`, `git diff --stat`,
`git diff --name-only`, `git ls-files`, `rg`, чтение файлов, разбор YAML/JSON/
TOML/Markdown, сравнение generated/source contracts и inspection доступных CI
результатов. Разрешён финальный `git diff --check` как отдельный whitespace
gate. Для ссылок используй текстовый поиск и ручное разрешение targets, не
запускай проектные documentation/link-test scripts.

## Checklist

- все пункты плана имеют code/wiring/evidence owner;
- нет незаполненных маркеров, пустых заготовок, dead branch или docs claim без реализации в scope;
- proto tags/types, migration order, bounded fields, permissions и error paths
  согласованы по обеим сторонам границы;
- workflow path filters и release-version files охватывают source changes;
- `docs/plans/README.md`, `docs/README.md` и канонические docs не ссылаются на
  удалённый plan-файл;
- `git diff --check` проходит;
- финальный diff не содержит секретов, токенов, PII, абсолютных машинных путей,
  кэшей или посторонних изменений.

Отсутствие локального запуска укажи явно: статический audit не является
доказательством runtime correctness; доказательство должно прийти из CI или
остаться `UNAVAILABLE/PENDING`.
