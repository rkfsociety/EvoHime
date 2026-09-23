# План 183.3 — Runtime и application crates

## Изменить

1. Документировать и добавить сценарные doc-tests к согласованным в этапе 1
   application/runtime crates: `tool-runtime`, `evohime-core`,
   `evohime-local-storage`, `evohime-listener`, `evohime-supervisor`,
   `evohime-updater`, `evohime-update-agent` и `evohime-cli`.
2. Описать public facade/re-export contracts, ownership и lifecycle,
   cancellation/timeout, persistence/error behavior, platform requirements,
   permission/approval boundaries и безопасные ограничения там, где они входят
   в API фактически.
3. Сверить публичные документы с актуальными tests, module boundaries и
   каноническими архитектурными документами; не документировать private
   implementation details как поддерживаемые контракты.
4. Завершить `missing_docs` coverage для публичной поверхности crate-by-crate;
   не применять blanket allow к generated, Windows-only или unstable items.

## Зависимости

### Блокирующие

- Завершённая inventory/матрица из этапа 183.1.
- Завершённые соответствующие этапы 183.2, чтобы общий lint/gate не возвращал
  уже исправленные пробелы.

### Опциональные

- Публичные CLI примеры могут ссылаться на примеры из `evohime-cli-protocol`,
  если они остаются автономно компилируемыми.

## Проверка

Запустить per-crate doc-tests и rustdoc с `--no-deps`, затем workspace
verification для всех targets, включая Windows-specific crates на поддерживаемом
runner. Просмотреть примеры на отсутствие реальных ключей, данных пользователя и
сетевых побочных эффектов.

## Rollback и evidence

Любой пример, требующий недетерминированной внешней службы, заменить на
детерминированный локальный пример либо явно пометить с причиной. Не скрывать
непроходящий rustdoc/doc-test lint через общий `allow`.
