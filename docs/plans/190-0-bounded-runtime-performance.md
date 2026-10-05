# 190-0 — Ограниченная память и производительность Core/Electron

Статус: план, реализация не выполнена. Дата: 2026-10-05.
Основание: выборочный статический аудит checkout `c89247ea4f748e2c387549e4f18984c3df83b0de`.

Повторно сверено с upstream `93857205` после fetch. Номер 189 уже закрыт.

## Цель и область

Устранить неограниченное накопление ключей событий чата, повторный опрос
workflow и синхронный файловый hot path диагностики. Сохранить выполненную
upstream lazy JSX оптимизацию и измерить оставшуюся подготовку transcript.
Проверить небольшую оптимизацию UTF-8
декодирования RAG. Не заявлять ускорение без сравнительных замеров.

Подтверждённые источники:

| Участок | Наблюдение | Этап |
| --- | --- | --- |
| `TaskTimeline.tsx`, `buildCoreEventKey`, `eventCursorRef` | Ключ включает payload/страницу, Set растёт до смены чата или reconnect | 3 |
| `WorkflowPanel.tsx`, polling effect | Каждые 2 секунды два запроса, `afterSequence: -1`; terminal state не завершает polling | 3 |
| `main/diagnostics/logger.ts`, `write` | mkdir/stat/append и ротация синхронны на вызывающем потоке | 2 |
| `TaskTimeline.tsx`, `timelineItems` | Upstream уже выбирает descriptors до JSX; проверить оставшиеся затраты transcript | 3 |
| `workspace_rag.rs`, `decode_text` | `String::from_utf8(bytes.to_vec())` копирует буфер до проверки UTF-8 | 1 |

Пути renderer/main относительны к `desktop/evohime-electron/src/`;
Rust-путь — к `crates/evohime-core/src/`.

## Зависимости и порядок

- Blocking: существующие authenticated IPC, conversation sequence/replay,
  workflow projections, SQLite pool и redaction; новых numbered dependencies нет.
- Optional: 184 (trace evaluation) и 185 (simulation harness); их реализация
  не нужна для локальных сравнительных измерений и regression tests.
- Порядок: [1](190-1-bounded-runtime-performance.md) →
  [2](190-2-bounded-runtime-performance.md) →
  [3](190-3-bounded-runtime-performance.md) →
  [4](190-4-bounded-runtime-performance.md).

## Ограничения

Core сохраняет владение SQLite, tools и durable events. Не менять proto,
SQLite schema, permissions, model routing, update channel или публичный API
без установленной необходимости. Не добавлять production-зависимости.
Не заменять `Arc::clone` механически. Не исправлять установленный продукт.
Не вводить worker/service/registry только ради этого плана.

Численные bounds очередей/кешей обосновать существующими limits и тестами до
реализации; количество элементов и суммарный объём памяти ограничивать отдельно.
Ограничение кеша не должно удалять durable историю или ослаблять replay recovery.

## Проверка, recovery и rollback

Сначала снять baseline на повторяемых fixtures, затем сравнить тот же сценарий
на той же машине/сборке: память долгого чата, подготовка длинной ленты,
число IPC-запросов workflow, задержка event loop при логировании и RAG decode.
Runtime-ускорение logger/ленты пока является гипотезой; рост Set и повторные
запросы подтверждены кодом. Тесты обязательны для корректности, замеры — для
утверждений об ускорении; нестабильные wall-clock пороги не включать в CI.

Recovery покрывает reconnect/epoch, replay, slow workflow response,
shutdown/ошибку записи логов. Rollback — возврат исходного кода затронутого
модуля обычным коммитом; миграции и очистка пользовательских данных не нужны.
Evidence хранить в `docs/release-evidence.md` только после фактических проверок.

## Результат ревью плана

Принято: bounded dedup, cursor polling, logger queue, проверка существующего lazy JSX, UTF-8 decode.
Отклонено: обещание численного ускорения без baseline; полная замена системы
виртуализации; изменение Core authority; массовая замена `Arc::clone`.
Внешних блокеров составления плана нет. Готовность реализации определяется
этапом 4, а не наличием этого документа.
