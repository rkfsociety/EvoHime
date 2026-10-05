# 190-3 — Bounded chat projection и workflow polling

## Зависимости

Blocking: [190-2](190-2-bounded-runtime-performance.md). Optional: нет.

## Изменения и контракт

### Дедупликация чата

В `TaskTimeline.tsx` заменить хранение полных payload/JSON страниц в ключах
компактной идентичностью с instance/epoch/sequence и conversation event IDs.
Проверить legacy/replay случаи: нельзя предполагать уникальность sequence
для всех envelopes или терять обновлённую projection с тем же event ID.
Не вводить короткий hash без collision handling.

Ограничить `seenKeys`, `pages`, `eventsById` и timestamp cache согласованно с
retained history/live bounds. Cursor metadata не должна удерживать удалённые
из projection payloads. Явно загруженная пользователем история может расти:
не выдавать её за leak и не удалять её молча. При eviction/reconnect/смене
epoch сохранить gap detection, catch-up и conflict semantics.

### Лента

В `TaskTimeline.tsx` сохранить уже реализованный upstream выбор descriptors
до создания JSX видимого окна/overscan. Не реализовывать это повторно.
Измерить оставшиеся затраты descriptors/transcripts; оптимизировать только
подтверждённый hot path. Сохранить stable keys, scroll anchoring, variable-height
Markdown, follow-live, older history, copy и optimistic retry. Переиспользовать
существующую виртуализацию; не добавлять библиотеку. Transcript кешировать
только по корректному revision/input, с bounded eviction; сначала измерить,
нужен ли кеш после переноса JSX за выбор диапазона.

### Workflow

В `WorkflowPanel.tsx` использовать существующий `afterSequence` для incremental
events; объединять страницы без потерь/дублей и догружать следующую страницу
при достижении limit. Курсор обновлять после получения подходящей projection,
а не после acknowledgement команды. Проверить фактическую корреляцию ответов
по run ID; старый ответ не меняет новый запуск.

Оставлять максимум один polling cycle в полёте, с bounded timeout/recovery
по существующему client contract. Terminal state останавливает polling лишь
после получения финальных событий. Reconnect возобновляет catch-up; unmount
и смена запуска убирают таймер и игнорируют устаревшие результаты.
Не добавлять новый IPC contract, если существующих projections достаточно.

## Verification и готовность

Расширить `tests/task-timeline.test.tsx`, `tests/workflow-panel.test.tsx` и
при необходимости projection tests. Проверить поток существенно длиннее
retention limit, memory bounds, duplicate/revised events, epoch/reconnect,
history paging, scrolling, slow/out-of-order responses, больше 200 workflow
events, terminal completion и timer cleanup. Использовать fake timers и
управляемые responses, без wall-clock assertions.

Запустить targeted tests, typecheck и check:protocol; перед закрытием —
полный Electron suite и bundle gates. Сравнить число IPC requests, peak cache
bytes и время подготовки длинной ленты с baseline на одинаковом fixture.

## Recovery, rollback и release

Core остаётся источником durable history/progress; UI восстанавливается через
существующий replay/catch-up. SQLite/schema и permissions неизменны.
Rollback — revert renderer changes; пользовательскую историю не очищать.
Повысить patch marker UI bundle; другие markers только при реальном изменении
их sources по module map. Сохранить фактические результаты в release evidence.
