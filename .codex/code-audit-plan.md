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
| Electron renderer panels/stores | в работе | Убраны копии newest-first event stream в transcript и PlanReview roster/failure projection; продолжить просмотр мемоизации, подписок, лимитов и устаревших ответов. |
| Rust Core | ожидает | Проверить горячие циклы, clone/serialize, async cancellation и event fan-out. |
| Storage/receipts/permissions/gateway | в работе | В `context_ledger.prune` устранён N+1 SELECT по receipt pin через `NOT EXISTS`; проверить остальные retention/receipt SQL-проходы. |
| Tools/listener/supervisor/updater | ожидает | Проверить bounded I/O, процессы, retry и очистку ресурсов. |
| C#/scripts/package/CI | ожидает | Проверить drift compatibility и воспроизводимость gates. |

## Правило завершения

Пункт отмечается завершённым только после просмотра соответствующего кода,
реализации либо обоснованного отказа, запуска focused tests и общей проверки
затронутого пакета. После каждого изменения выполняются `git diff --check`,
task-only commit и разрешённый для этой задачи push с проверкой удалённого SHA.
