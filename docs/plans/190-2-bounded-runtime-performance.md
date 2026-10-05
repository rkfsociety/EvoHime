# 190-2 — Ограниченная асинхронная диагностика Electron

## Зависимости

Blocking: [190-1](190-1-bounded-runtime-performance.md). Optional: нет.

## Изменения и контракт

Расширить существующий `JsonlLogger` в
`desktop/evohime-electron/src/main/diagnostics/logger.ts` и его lifecycle
в `main/index.ts`; не создавать второй logger.

- Инициализировать каталог/размер один раз; сохранить текущую redaction до
  попадания записи в очередь и сериализацию JSONL.
- Перейти к последовательному async append/rotation с bounds по числу записей
  и bytes; сохранить порядок, maxBytes/maxFiles и корректную ротацию.
- Явно определить overflow policy: без бесконечного ожидания, неограниченной
  памяти и silent loss. Счётчик потерь/состояние ошибки доступны диагностике;
  запись сигнала о переполнении не должна рекурсивно перегружать logger.
- Завершение shell выполняет bounded drain/flush. Ошибка filesystem не
  завершает shell и не вызывает бесконечных retry; disabled/error состояние
  наблюдаемо. Не превращать диагностику в durable audit authority.
- Проверить всех потребителей logger: асинхронное завершение учитывается
  тестами и export support bundle, где нужна свежая запись.

При crash возможна потеря очереди: её верхняя граница должна быть документирована.
Если существующий контракт требует синхронной записи критических событий,
сохранить этот узкий путь и измерить его отдельно. Секреты не писать в evidence.

## Verification и готовность

Найти существующие tests по `JsonlLogger`; проверить redaction, порядок,
rotation, concurrent writes, queue saturation, filesystem failure и shutdown.
Использовать временные каталоги и управляемый slow writer, без UAC.
Запустить affected tests и Electron typecheck. Измерить event-loop latency
и queue peak при том же потоке записей, что в baseline; указать p50/p95,
bytes, dropped count и длительность flush. Ускорение не заменяет correctness.

## Recovery, rollback и release

Startup открывает текущий log с корректным размером, crash не смешивает
ротации и не повреждает старые generations. IPC/schema не меняются.
Rollback — revert logger/lifecycle, без удаления логов. Повысить patch marker
затронутого публикуемого модуля по module map, сохранить evidence этапа.
