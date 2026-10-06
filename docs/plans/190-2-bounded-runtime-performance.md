# 190-2 — Ограниченная асинхронная диагностика Electron

## Зависимости

Blocking: [190-1](190-1-bounded-runtime-performance.md). Optional: нет.

## Изменения и контракт

Расширить существующий `JsonlLogger` в
`desktop/evohime-electron/src/main/diagnostics/logger.ts` и его lifecycle
в `main/index.ts` и `main/updater.ts`; не создавать второй logger.

Обе Electron entry points сейчас создают независимый `JsonlLogger` для
одинакового `shell-main.jsonl`. До перехода на async определить и проверить
контракт владения/сериализации для общего пути: одновременные append и rotation
не должны терять записи, переставлять поколения или портить JSONL. Lifecycle
и закрытие очереди определить отдельно для обоих entry points; нельзя считать,
что `main/index.ts` управляет shutdown updater entry point.

- Инициализировать каталог/размер один раз; сохранить текущую redaction до
  попадания записи в очередь и сериализацию JSONL.
- Перейти к последовательному async append/rotation с bounds по числу записей
  и bytes; сохранить порядок, maxBytes/maxFiles и корректную ротацию.
- Явно определить overflow policy: без бесконечного ожидания, неограниченной
  памяти и silent loss. Счётчик потерь/состояние ошибки доступны диагностике;
  запись сигнала о переполнении не должна рекурсивно перегружать logger.
- Завершение каждого shell entry point выполняет bounded drain/flush. Учесть,
  что обработчик Electron `before-quit` не ожидает возвращённый Promise:
  lifecycle должен явно задержать завершение до flush и безопасно продолжить
  выход после успеха/таймаута, не зацикливая повторный quit. Контролируемые
  `app.exit` пути также выполняют bounded flush; при его неудаче отправляют
  ограниченное сообщение в `stderr` и продолжают выход. Ошибка filesystem не
  завершает shell и не вызывает бесконечных retry; disabled/error состояние
  наблюдаемо. Не превращать диагностику в durable audit authority.
- Проверить всех потребителей logger: асинхронное завершение учитывается
  тестами и export support bundle, где нужна свежая запись; bundle не читает
  файл в состоянии частичной записи и получает определённый snapshot/drain.

При crash возможна потеря очереди: её верхняя граница должна быть документирована.
Если существующий контракт требует синхронной записи критических событий,
сохранить этот узкий путь и измерить его отдельно. Секреты не писать в evidence.

## Verification и готовность

Создать focused `JsonlLogger` tests (в текущем Electron tests отдельного набора
не найдено): проверить redaction, порядок, rotation, concurrent writes,
совместный доступ entry points, queue saturation, filesystem failure, bundle
snapshot и shutdown обоих entry points. Использовать временные каталоги и
управляемый slow writer, без UAC.
Запустить affected tests и Electron typecheck. Измерить event-loop latency
и queue peak при том же потоке записей, что в baseline; указать p50/p95,
bytes, dropped count и длительность flush. Ускорение не заменяет correctness.

## Recovery, rollback и release

Startup открывает текущий log с корректным размером, crash не смешивает
ротации и не повреждает старые generations. IPC/schema не меняются.
Поскольку общий logger попадает в shell-host и updater packages, покрыть
`src/main/diagnostics/logger.ts` path filters обоих workflows и повысить patch
markers обоих реально изменённых модулей. Rollback — revert logger/lifecycle,
без удаления логов; сохранить evidence этапа.
