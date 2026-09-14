# План 172.2 — Startup preflight, self-update и crash recovery

Статус: этап 2 для [плана 172.0](./172-0-updater-recovery-and-self-healing.md).

## Зависимости

### Блокирующие

- План 172.0 и этап 172.1.
- Existing `--launch`, `--check`, `--apply` modes.
- Existing transaction worker apply/rollback/health contract.
- Existing `EvoHimeUpdater.exe` and `EvoHime.exe` launch paths.

### Опциональные

- Existing updater recovery PowerShell bridge as manual last resort.
- Existing shell reload limiter and update failure diagnostics.

## Реализация

1. Сделать Rust updater первым запуском из desktop shortcut и post-install
   path. Его `--launch` сначала выполняет bounded preflight, а затем передаёт
   управление существующему Electron updater UI. Это не меняет пользовательское
   окно и не добавляет новый модуль.
2. В preflight реализовать порядок:
   `recover journal -> validate active updater -> validate transaction worker ->
   read compatibility -> repair control plane -> repair selected modules ->
   self-test -> launch UI/shell`.
3. При updater self-update запускать temporary copy текущего updater, ждать
   освобождения файлов, переносить active slot, проверять новый процесс через
   `--self-test` и только после успеха фиксировать commit. При отказе запускать
   last-known-good slot и оставлять диагностическую причину.
4. Если transaction worker отсутствует или не проходит self-test, сначала
   заменить только его verified artifact самим updater’ом. После этого обычные
   shell-host/ui/native операции снова выполняются существующей
   transaction-транзакцией с backup и health-check.
5. Если Electron updater UI/app.asar повреждён, выполнить headless repair
   shell-host/ui-bundle через существующий module path. Не запускать UI до
   успешной валидации. Если UI всё ещё не стартует, не делать бесконечный
   restart: сохранить bounded failure state и предложить ручной
   `repair-updater.ps1`/полный installer только как последний fallback.
6. Добавить crash-loop protection для updater UI и shell: bounded счётчик
   неудачных стартов, окно времени, rollback последнего shell/ui apply и
   переход в `manual-recovery` после исчерпания попыток.
7. Сделать скачивание module artifacts resumable/retry-safe: временный файл,
   повтор после transient HTTP/network failure, удаление hash mismatch и
   повтор с нуля, отсутствие установки до полной проверки artifact.

## Критерии выхода

- [ ] Сломанный Electron updater больше не блокирует запуск Rust recovery path.
- [ ] Сломанный updater восстанавливается из last-known-good slot.
- [ ] Сломанный transaction worker восстанавливается без полного installer.
- [ ] Self-update не может оставить активным непроверенный бинарник.
- [ ] UI/shell crash-loop ограничен и откатывается один раз по journal.
- [ ] Network/partial/hash failures имеют bounded retry и понятный status.
- [ ] Existing user data, Core state и credentials не участвуют в repair copy.

## Не входит

Новая служба, новый scheduler, web UI, source rebuild, автоматический commit/
push или изменение Core/SQLite authority.
