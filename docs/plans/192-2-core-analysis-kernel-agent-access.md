# План 192.2 — Core runtime-интеграция и recovery

Статус: active implementation contract. Этап следует после 192.0 и 192.1.

## Scope и изменяемые контракты

Подключить `analysis.run` к существующему kernel runtime и сделать сессии,
созданные агентом, обычными Core-owned сессиями — с теми же лимитами,
идемпотентностью и recovery, что и у IPC-команд.

1. Ввести в `evohime-core` единый владелец `analysis_kernels` для
   coordinator-пути. Сейчас map живёт только в `IpcBridge`
   (`ipc_bridge_header.rs:656`). Нужен Core-owned host-объект с тем же
   содержимым, чтобы агент и IPC-команды обращались к одним и тем же
   runtime, а не к двум независимым map. `IpcBridge` переиспользует этот
   объект, а не держит собственный `HashMap` рядом.
2. Реализовать `KernelHost` из 192.1 поверх этого владельца и подключить его
   к реестру через `ToolRegistry::with_kernel_host`:
   - ленивое создание сессии на первый вызов в задаче (`create-on-demand`)
     с детерминированным id и `task_id`/`workspace_id` из `ToolContext`;
     `ToolContext` не меняется;
   - `package_manifest_hash` и `policy_hash` вычисляются Core из
     `runtime_version = "trusted-local-1"` и канонической декларации
     инструмента; модель их не передаёт;
   - прямое переиспользование процесса: `KernelRuntime::admit` →
     существующий supervisor `kernel_execute` (Windows, при наличии
     `EVOHIME_LAUNCH_CONTEXT`) либо локальный `KernelRuntime::execute`;
   - `object_put` кладёт объект в ephemeral память runtime и возвращает
     bounded `KernelObjectRefV1` без значения.
3. Ограничения размера: tool input ≤ лимита из `KernelLimitsV1`
   (`cpu_time_ms`, `output_bytes`), ответ bounded по
   `KERNEL_MAX_RESULT_BYTES`; при превышении — typed `limit_exceeded`, runtime
   переводится в `LimitExceeded` (существующее поведение `accept_output`).
4. Чувствительные inline-аргументы по-прежнему отсекаются существующим
   `contains_sensitive_marker`; ничего не добавляем в обход.
5. Timing: kernel-сессия живёт в рамках задачи, а не процесса Core. На
   завершении задачи (или по `idle_timeout_ms`/`lifetime_timeout_ms`) runtime
   останавливается существующим `kernel_stop`, а в SQLite остаётся только
   metadata с финальным статусом. Отдельный sweep-механизм не вводится: это
   ответственность существующего recovery.
6. Recovery: `recover_analysis_kernels` уже fencing-ит running-сессии в
   `Crashed`; agent-path не должен обходить это и повторять blind retry.
   Повторный `analysis.run` после краха создаёт новую сессию только явным
   следующим шагом агента, который видит typed `crashed`/`unavailable`.
7. Idempotency: `idempotency_key` формируется Core из
   `(task_id, kernel_id, operation, request_hash)`; повтор одного и того же
   вызова не исполняется второй раз и возвращает существующий исход.

## Зависимости

### Блокирующие

Закрытый план 28 (runtime, worker, лимиты, recovery); этап 192.1;
существующий `KernelRuntime`/`AnalysisKernelStore` API
(`create_session`, `set_status`, `list_objects`, `append_event`,
`get_idempotency`).

### Опциональные

Публикация крупного результата в `ArtifactStore` вместо inline — желательно,
но `object_put` уже даёт bounded-путь без неё.

## Recovery и rollback

Отказ worker → `mark_crashed`, typed `unavailable`, процесс очищается.
Превышение лимита → `LimitExceeded`, объекты очищаются, сессия требует
`reset`. Рестарт Core → running-сессии становятся `Crashed`, blind retry
запрещён. Rollback: убрать host-реализацию и объявление инструмента (192.1);
kernel IPC-команды, storage и worker остаются работоспособными, история
сессий сохраняется. Схема SQLite не меняется.

## Verification

Unit: первый вызов создаёт сессию с детерминированными хешами и task id;
второй вызов переиспользует её; превышение `output_bytes` даёт
`limit_exceeded` и `Reset`-able состояние; чувствительный inline-аргумент
даёт `sensitive_inline_payload`; повтор `idempotency_key` не исполняется
дважды. Recovery: после симуляции рестарта running-сессия — `Crashed`, а
новый вызов создаёт новую сессию. Integration: два параллельных вызова в
одной задаче не создают две сессии.

## Release evidence

Зафиксировать: id/версию kernel-инструмента, mapping операций, факт
переиспользования `KernelLimitsV1`, идентификатор idempotency-схемы,
результат recovery-теста, отсутствие изменений SQLite-схемы и запуска
дополнительных worker'ов вне существующего supervisor-контракта. Не сохранять
значения объектов.

## Критерии выхода

- [ ] Агент получает результат `json_parse`/`json_select`/`csv_summary` без
      значений в trace.
- [ ] Сессия создаётся один раз на задачу и переиспользуется.
- [ ] Повтор с тем же `idempotency_key` не исполняется дважды.
- [ ] Превышение лимита даёт typed ошибку и требует явного reset.
- [ ] Рестарт Core не порождает blind retry.
- [ ] Внутренние ссылки разрешаются, `git diff --check` проходит.
