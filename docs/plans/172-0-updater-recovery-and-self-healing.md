# План 172.0 — Self-healing updater и recovery без нового модуля

Статус: предложено по запросу Романа. Это implementation contract; функционал
этим документом не считается реализованным.

## Цель

Сделать существующий `evohime-updater.exe` самовосстанавливающимся control-plane
для EvoHime. Ошибки updater, transaction worker, Electron updater UI и
модульной поставки должны исправляться через уже опубликованные module
releases и `compatibility` manifest без пересборки полного
`EvoHime-Setup.exe`.

## Архитектурная граница

```text
desktop shortcut
  -> existing Rust updater recovery entrypoint
  -> preflight / control-plane repair
  -> existing Electron updater UI
  -> existing transaction worker for normal component transactions
```

План расширяет существующие `evohime-update-agent` и
`evohime-updater`/transaction contracts. Новый runtime-модуль, отдельный
release channel, web-runtime, HTTP-сервер или web-инсталлятор не создаются.
Recovery-slot является второй постоянной копией того же updater artifact, а
не новым публикуемым модулем: module release по-прежнему содержит один
`evohime-updater.exe`.

## Failure matrix

| Сбой | Автоматическое действие | Полный installer |
| --- | --- | --- |
| Сетевой/HTTP/partial download | bounded retry, очистка `.part`, повторная hash-проверка | нет |
| Неполный или битый compatibility manifest | остановка без записи, повтор позже | нет |
| Устаревший updater | скачать updater module, атомарно заменить и проверить | нет |
| Сломанный текущий updater | запустить persistent last-known-good slot | нет |
| Сломанный transaction worker | прямой verified repair control-plane файлом updater | нет |
| Сломанный Electron updater UI/app.asar | headless preflight чинит shell/ui, затем запускает UI | нет |
| Частичная native/UI транзакция | существующий journal + backup + rollback | нет |
| Потеря обеих recovery-копий или install tree | показать manual recovery | да |

## Блокирующие зависимости

- Component manifest, compatibility manifest и module releases плана 144.
- Существующий `evohime-update-agent` network/verification path.
- Существующий transaction journal, backup, health-check и rollback.
- Существующие Windows packaging, shortcut и installer smoke contracts.
- Existing updater status JSON и Electron updater UI.

## Опциональные зависимости

- Diagnostics/support bundle и bounded update-failure issue.
- Дополнительные accessibility/visual проверки updater window.
- CI performance evidence для module-only публикации.

## Критерии готовности

- [ ] Запуск начинается с существующего Rust updater recovery entrypoint.
- [ ] Обновление updater и transaction worker не требует полного installer.
- [ ] Для updater существует persistent last-known-good slot с journal и rollback.
- [ ] Новый бинарник принимается только после размера, SHA-256, PE и self-test.
- [ ] Падение UI или crash-loop не превращается в бесконечный перезапуск.
- [ ] Обычные native/UI обновления сохраняют существующий transaction rollback.
- [ ] Module-only CI/release path подтверждён без запуска полного installer.
- [ ] Tests, package smoke, release evidence и canonical docs обновлены.

## Не входит

Новый модуль или новый канал поставки, отдельный web-продукт, автоматический
source rebuild на машине пользователя, отключение CI/security gates,
изменение Core ownership, silent rollback пользовательских данных и
автоматическое изменение установленного клиента вне штатного update flow.

## Этапы

- [Этап 1 — recovery state и verified control-plane repair](./172-1-updater-recovery-and-self-healing.md)
- [Этап 2 — startup preflight, self-update и crash recovery](./172-2-updater-recovery-and-self-healing.md)
- [Этап 3 — IPC/status и updater UI](./172-3-updater-recovery-and-self-healing.md)
- [Этап 4 — verification, release evidence и закрытие](./172-4-updater-recovery-and-self-healing.md)
