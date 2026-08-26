# EvoHime — план полного прохода кода

Статус: выполняется с 26 августа 2026 года.

Цель — пройти текущий checkout по всем исполняемым пакетам, найти улучшения,
которые подтверждаются кодом или измерением, реализовать безопасные пункты,
добавить регрессионные проверки и зафиксировать результат task-only коммитами.
Функциональные этапы 01–21 не переоткрываются без доказанного нарушения их
контракта.

## Порядок аудита

1. Electron main/preload/renderer и IPC adapter.
2. Rust Core: основной цикл, события, workflow, workspace/RAG и память.
3. Rust storage, receipts, permissions и model gateway.
4. tool-runtime, listener, supervisor и updater.
5. C# compatibility shell, PowerShell scripts, package/CI и документация.

Для каждого участка проверяются: лишние аллокации и повторные проходы,
границы размеров/таймаутов/отмены, ошибки и восстановление, безопасность
ввода/вывода, тестовое покрытие и соответствие каноническим документам.

## Журнал

| Участок | Состояние | Результат |
| --- | --- | --- |
| Electron transcript/task timeline | завершено | Убран clone+reverse потока и повторное сканирование событий по каждому сообщению; добавлен тест сохранения порядка. Коммит `6264073c`. |
| Electron main/preload и IPC | завершено | Проверены очереди, replay, frame parsing, backoff и lifecycle; `LedgerEventDedup` переведён с O(n) `shift()` на O(1) ring eviction, добавлены 2 focused tests. |
| Electron renderer panels/stores | завершено | Убраны копии newest-first event stream в transcript и PlanReview roster/failure projection; `OperationsPanel` переведён на один memoized pass; форматтер времени `TaskTimeline` кэшируется на уровне модуля; проверены мемоизация, подписки, лимиты и stale-response guards. |
| Rust Core | завершено | Проверены горячие циклы, clone/serialize, async cancellation, event fan-out и bounded collections. `context-budget::normalize_text` переведён на однопроходное построение без промежуточного line vector. |
| Storage/receipts/permissions/gateway | завершено | В `context_ledger.prune` устранён N+1 SELECT по receipt pin через `NOT EXISTS`; остальные retention/receipt SQL-проходы, permissions и gateway bounds проверены. Малые bounded `Vec::remove(0)` оставлены из-за slice API или малых жёстких лимитов. |
| Tools/listener/supervisor/updater | завершено | В `TelemetryBuffer::export_jsonl` убрана промежуточная коллекция строк и второй проход `join`; `PulseDigest` считает события одним проходом; проверены bounded I/O, процессы, retry, cancellation и очистка ресурсов. |
| C#/scripts/package/CI | завершено | Compatibility tests, PowerShell/package smoke, protocol/typecheck/build/bundle и release-аудит проверены; drift не найден. |

## Итог прохода

Подтверждённые улучшения реализованы отдельными task-only коммитами:

- Electron: `1534030`, `41799eb`, `b605442`, `793dfbf`, а также ранее
  `6264073`.
- Rust: `bce9c399`, `ead43ed`, `c38f286`, `218801a`.

Потенциальные изменения, которые сознательно не внесены: bounded replay и
telemetry buffers с публичным `&[T]` API, а также SSE parser с потоковым
буфером. Для них переписывание контейнера или индексации увеличило бы риск и
не дало подтверждённого выигрыша при текущих лимитах.

Проверка 26 августа 2026 года: `cargo fmt --all -- --check`, строгий workspace
`cargo clippy`, `cargo test --workspace --all-targets`, Electron `npm test`
(452 passed, 2 skipped), protocol/typecheck/build/check:bundle, C# compatibility
tests, native-package smoke и `scripts/final-release-audit.tests.ps1` — успешно.

## Правило завершения

Пункт отмечается завершённым только после просмотра соответствующего кода,
реализации либо обоснованного отказа, запуска focused tests и общей проверки
затронутого пакета. После каждого изменения выполняются `git diff --check`,
task-only commit и разрешённый для этой задачи push с проверкой удалённого SHA.
