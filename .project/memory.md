# Память проекта EvoHime

Это краткий индекс подтверждённых неочевидных сведений для будущих задач.
Источником истины остаются исходники, тесты и документы проекта.

## Runtime и владение

- `evohime-core` владеет runtime-состоянием и SQLite; Electron renderer не
  должен получать это состояние напрямую. Подробный контракт находится в
  [`docs/architecture.md`](../docs/architecture.md).
- `Arc::clone` увеличивает счётчик ссылок и сам по себе не копирует объект.
  В Core его нельзя механически заменять на `Arc::as_ref()`: async-задачи,
  `tokio::spawn` и передача владения между обработчиками требуют собственного
  `Arc`. Устранять следует подтверждённые копии содержимого (`String`, `Vec`,
  крупные структуры), сохраняя границы владения и время жизни lock guard.

## Рабочий процесс

- Подтверждённое текущее состояние checkout и release evidence хранятся в
  [`docs/current-state.md`](../docs/current-state.md) и
  [`docs/release-evidence.md`](../docs/release-evidence.md); не копируй их
  таблицы сюда.
- Для очистки сгенерированных игнорируемых артефактов, если автоматическая
  проверка блокирует `rm -rf`, сначала проверь точный путь командой
  `git clean -ndX -- <path>`, затем удали только показанные игнорируемые файлы
  командой `git clean -fdX -- <path>`. Флаг `-X` ограничивает очистку
  игнорируемыми файлами; продолжай только если dry-run перечисляет ожидаемый
  build output. Этот способ проверен на `desktop/evohime-electron/out`.

## Модульная поставка и сервисы

- Канонический реестр всех 26 модулей продукта находится в
  [`modules.manifest.json`](../modules.manifest.json); скрипт
  `scripts/publish-compatible-manifest.ps1` читает список модулей из него,
  а `scripts/check-modules-manifest.ps1` проверяет соответствие версий и
  workflow-файлов.
- Декларативный пакет `core-assets` (`core-assets.zip`) распаковывается
  `evohime-update-agent` в `assets/core-assets` (отдельно от
  `UpdateTransaction::SELECTABLE_COMPONENTS`), а сервисы с
  `restart = "service:<id>"` обновляются без закрытия окна приложения.
