# Этап 04.2: Хранение транскриптов и retention

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: этап 04.1 — лимиты, состояния и схема политики приходят оттуда.

Разблокирует: 04.3–04.7.

## Что этап отдаёт наружу

Core-owned хранилище эпизодов и высказываний с retention, удалением и
tombstone, плюс персистентная политика ambient.

## Что уже есть в коде

`SCHEMA_VERSION = 24` в `crates/evohime-local-storage/src/lib.rs:29`;
последняя миграция (v24, `lib.rs:2903`) добавила `coordinator_child_checkpoint`
и `child_parent_sequences` для child workflows. Механика миграций —
единственная функция `migrate` (`lib.rs:2158`): цепочка блоков
`if current < N { … PRAGMA user_version = N; }` внутри одной транзакции, с
бэкапом базы до применения и восстановлением из него при сбое (тест
`restores_backup_when_migration_fails`, `lib.rs:3374`). `foreign_keys`
включается при открытии соединения (`lib.rs:377`), поэтому `ON DELETE CASCADE`
действительно работает. Образцом для стора служат `memory_store.rs`
(tombstone, forget, expiry) и `context_ledger_store.rs`. Таблиц ambient нет.

Сверено дополнительно, потому что этап на это опирается:

- `append_event(task_id: &str, event_type: &str, payload: &[u8])`
  (`lib.rs:1837`) — первый аргумент называется `task_id`, а не «subject», и он
  `NOT NULL`; payload хранится как **BLOB**. Индекс —
  `idx_events_task_sequence ON events(task_id, sequence_id)`;
- `push_journal_tail` определён в `crates/evohime-core/src/ipc_bridge.rs:208`
  и вызывается из `pipe_server.rs:158`; курсор клиента — `sequence_id > last`;
- **сегодня из `events` не удаляется ни одна строка.** Даже очистка истории
  ревью сделана маркерным событием `review.history_cleared` (`lib.rs:1991`), а
  не `DELETE`;
- `spawn_receipt_retention` (`lib.rs:3698`) — `tokio::spawn` с
  `loop { sleep(6h); … }`, берёт `EventJournal` по значению, возвращает
  `JoinHandle<()>`, `CancellationToken` **не** использует. Важно: `sleep` стоит
  **до** первой итерации, поэтому копия этого паттерна при старте Core ничего
  не чистит;
- `purge_expired_backups(directory, retention_ms, now_unix_ms)`
  (`backup.rs:245`) уже вызывается прямо из обработчика forget памяти
  (`crates/evohime-core/src/lib.rs:8378`) с продовой константой
  `FORGET_BACKUP_RETENTION_MS = 7 * DAY_MS`
  (`memory_extraction.rs:60`) — то есть ротация бэкапов при удалении в этом
  коде уже связана с forget;
- `memory_entries.provenance_source_id` существует и проиндексирован
  (`lib.rs:2511`, `:2523`), но заполняется функцией
  `memory_provenance_source_id` (`lib.rs:10000`) только из
  `RawEvidenceLocator`: `message_id`, `tool_call_id`, `task_id`, `file_path`;
- `atomic_write_json` в `crates/evohime-receipts/src/key_lifecycle.rs:1620`
  **приватна** и возвращает `KeyError`; импортировать её нельзя.

## Содержание

- Миграция **v25** (после текущей v24; таблица предложений из 04.7 добавляется
  отдельной v26-миграцией) — новый блок `if current < 25` в той же функции
  `migrate`:

```sql
CREATE TABLE ambient_episodes (
  episode_id TEXT PRIMARY KEY NOT NULL,
  started_at TEXT NOT NULL, ended_at TEXT,
  utterance_count INTEGER NOT NULL, speech_ms INTEGER NOT NULL,
  engine_version TEXT NOT NULL, model_id TEXT NOT NULL,
  extraction_state TEXT NOT NULL CHECK(extraction_state IN
    ('disabled','pending','done','failed')),
  expires_at TEXT NOT NULL
);
CREATE TABLE ambient_utterances (
  utterance_id TEXT PRIMARY KEY NOT NULL,
  episode_id TEXT NOT NULL REFERENCES ambient_episodes(episode_id) ON DELETE CASCADE,
  sequence INTEGER NOT NULL,
  started_at TEXT NOT NULL, duration_ms INTEGER NOT NULL,
  text TEXT NOT NULL, text_hash TEXT NOT NULL,
  language TEXT NOT NULL, avg_logprob REAL NOT NULL,
  speaker TEXT NOT NULL,               -- v1 всегда 'unverified'
  redacted INTEGER NOT NULL DEFAULT 0,
  expires_at TEXT NOT NULL,
  UNIQUE(episode_id, sequence)
);
CREATE TABLE ambient_tombstones (
  tombstone_id TEXT PRIMARY KEY NOT NULL, episode_id TEXT NOT NULL,
  removed_at TEXT NOT NULL, reason TEXT NOT NULL, utterance_count INTEGER NOT NULL,
  expires_at TEXT NOT NULL,
  UNIQUE(episode_id, removed_at)
);
CREATE INDEX idx_ambient_utterances_episode ON ambient_utterances(episode_id, sequence);
CREATE INDEX idx_ambient_expiry ON ambient_utterances(expires_at);
CREATE INDEX idx_ambient_episode_expiry ON ambient_episodes(expires_at);
CREATE INDEX idx_ambient_tombstone_expiry ON ambient_tombstones(expires_at);
```

  Колонок для аудио нет **по конструкции**. Включённый `foreign_keys` не
  допускает осиротевших высказываний. У tombstone есть собственный
  `expires_at`, иначе «отдельный срок хранения» ниже нечем исполнить.
- Удаление выполняется транзакционно: сначала фиксируется metadata-only
  tombstone с количеством удаляемых высказываний, затем удаляются utterances и
  episode. `purge_expired` также создаёт bounded tombstone для удалённого
  эпизода; tombstones не содержат текст и сами истекают по `expires_at`
  (не дольше 30 дней) в том же purge-цикле. При частичном истечении utterances
  счётчики episode пересчитываются в той же транзакции.
- `crates/evohime-local-storage/src/ambient_store.rs`: вставка высказывания,
  открытие и закрытие эпизода, выборка эпизодов и высказываний с лимитами,
  дедупликация по `text_hash` в окне, удаление по эпизодам и по временному окну
  с tombstone, `purge_expired`.
- Retention: `EVOHIME_AMBIENT_RETENTION_DAYS` (по умолчанию 7, потолок 90) для
  текста, 30 дней для метаданных эпизодов; purge при старте Core и раз в час
  фоновой задачей. Образец — `spawn_receipt_retention` (`lib.rs:3698`), но с
  одной обязательной поправкой: там `sleep` стоит **перед** работой, поэтому
  стартовый purge вызывается явно **до** входа в цикл, а не подразумевается.
  `CancellationToken` в этих задачах сегодня не используется; ambient-purge
  либо повторяет существующий паттерн, либо вводит отмену — но тогда как
  осознанное изменение, а не «как у остальных».
- `ambient-policy.json` в data dir: атомарная запись через временный файл и
  rename. `atomic_write_json` из `key_lifecycle.rs` приватна и завязана на
  `KeyError`, поэтому переиспользовать её нельзя — либо она поднимается в
  общий модуль, либо ambient пишет свою по тому же рецепту:
  `create` → `write_all` → `sync_all` → `harden_path` → `rename` →
  `harden_path`. Owner-only ACL (`harden_path`) — часть образца, а не
  необязательное украшение. Повреждённый файл читается как дефолтная политика
  **с включённой паузой** — fail-safe в пользу тишины.
- Удаление эпизода отклоняет memory-кандидатов с
  `provenance_source_id = episode_id` причиной `source_deleted` — тем же
  приёмом, что `forget_with_tombstone` в `memory_store.rs:831`, и по
  существующему индексу `memory_entries(provenance_source_id)`. Условие
  работоспособности: сегодня это поле заполняется только из
  `RawEvidenceLocator` (`memory_provenance_source_id`, `lib.rs:10000`), где
  ambient-эпизода нет. Поэтому 04.6 обязан класть `episode_id` в одно из
  существующих полей локатора или расширить локатор — иначе связь «кандидат ↔
  эпизод» физически не возникнет и это правило удаления будет холостым.
- **Остаточное окно в бэкапах.** `create_backup` снимает копию базы перед
  миграцией, а `purge_expired_backups` (`backup.rs:245`) удаляет контейнеры
  старше переданного срока. Удаление ambient-эпизода повторяет то, что уже
  делает forget памяти (`lib.rs:8378`): вызывает `purge_expired_backups` с
  `FORGET_BACKUP_RETENTION_MS` (7 суток, продовая константа). Это чистит
  **только состарившиеся** контейнеры: в бэкапе моложе семи суток транскрипт
  физически остаётся на диске. Это окно называется явно в UI-тексте про
  удаление и в `docs/architecture.md`, а не заметается под формулировку
  «удалено безвозвратно».
- Ошибка вставки (диск заполнен, блокировка или другая ошибка SQLite) не
  ретраится листенером: Core возвращает `storage_failed`, не создаёт ложную
  запись и публикует `ambient.storage_error` в UI. Листенер помечает исходное
  высказывание как `dropped` и продолжает работу со следующим сегментом.
- **Ambient-события и durable journal.** События публикуются через
  `append_event(task_id, event_type, payload)` в таблицу `events`, откуда их
  вычитывают клиенты (`push_journal_tail`, `ipc_bridge.rs:208`). Таблица
  **append-only и не имеет retention вообще** — ни purge, ни срока. Значит
  наивная публикация `ambient.transcript`/`ambient.state` оставляет вечный
  след: список `episode_id` с числом высказываний и полную хронологию того,
  когда пользователя слушали, — причём этот след переживает
  `DeleteAmbientTranscripts` и `forget_window`. Правило этапа:
  1. в ambient-событиях нет ни текста, ни `text_hash`;
  2. `task_id` ambient-события — это `episode_id` (для событий без эпизода —
     стабильный ключ `ambient-session`). Иначе удаление пришлось бы делать
     сканом BLOB-payload; с этим соглашением оно идёт по существующему индексу
     `idx_events_task_sequence`;
  3. `ForgetAmbientWindow` и удаление эпизодов в той же транзакции удаляют из
     `events` строки с `event_type LIKE 'ambient.%'`, попадающие в окно по
     `created_at` или относящиеся к удалённому `episode_id`;
  4. для `ambient.state` вводится собственный срок хранения, равный retention
     метаданных эпизода (30 дней), и он вычищается тем же purge-циклом.

  Это первый в кодовой базе `DELETE` из `events` — до сих пор оттуда не
  удаляли ничего, даже при очистке истории ревью. Для читателей журнала это
  безопасно: курсор `push_journal_tail` монотонен по `sequence_id` и дырки
  переносит, — но факт первого прецедента фиксируется явно, в том числе в
  `docs/architecture.md`. Без всего этого критерий «забыть последние N минут»
  не выполняется.
- `forget_window(minutes)` удаляет высказывания с `started_at` в замкнутом
  окне `[now - minutes, now]` и отклоняет производные кандидаты. Эпизод,
  начавшийся до окна, не удаляется целиком только из-за того, что пересекает
  его границу; после удаления счётчики обновляются, а пустой эпизод удаляется
  в той же транзакции.

## Файлы

- изменить: `crates/evohime-local-storage/src/lib.rs` (миграция v25,
  `SCHEMA_VERSION = 25`, экспорт модуля);
- создать: `crates/evohime-local-storage/src/ambient_store.rs`;
- изменить: `crates/evohime-core/src/lib.rs` (purge-задача со стартовым
  прогоном, удаление ambient-строк из `events`, ротация бэкапов при удалении);
- изменить: `docs/architecture.md` (остаточное окно бэкапов, срок ambient-строк
  в journal).

Правки CI не требуются: оба крейта (`evohime-local-storage`, `evohime-core`)
уже перечислены в строке `cargo test` в `.github/workflows/windows.yml:96`.

## Проверки

- миграция с v24 на v25 (и upgrade с каждой поддерживаемой более ранней версии)
  не теряет данные и идемпотентна; при искусственном сбое миграции база
  восстанавливается из бэкапа, как в существующем тесте
  `restores_backup_when_migration_fails`;
- `PRAGMA table_info` для ambient-таблиц не содержит BLOB-колонок;
- retention удаляет ровно просроченное и оставляет tombstone; просроченный
  tombstone тоже удаляется;
- стартовый purge отрабатывает до первого `sleep`: база, открытая с
  просроченными строками, чиста сразу после запуска Core, а не через час;
- удаление эпизода атомарно сохраняет tombstone до каскадного удаления и не
  оставляет осиротевших кандидатов памяти или ссылок;
- `forget_window(minutes)` удаляет высказывания в окне и отклоняет производных
  кандидатов;
- после `forget_window` и после `DeleteAmbientTranscripts` в таблице `events`
  не остаётся `ambient.*`-строк, ссылающихся на удалённые эпизоды, а клиент с
  курсором `sequence_id` продолжает чтение через образовавшуюся дырку;
- `ambient.*`-события ни при каких входных данных не содержат `text` или
  `text_hash`;
- повреждённый `ambient-policy.json` даёт дефолт с включённой паузой, а
  записанный файл имеет owner-only ACL.

## Критерии готовности

- транскрипты хранятся, читаются, истекают и удаляются под потолками;
- удаление не оставляет следа в durable event journal, а остаточное окно в
  backup-контейнерах названо пользователю честно;
- удаление источника не оставляет памяти-сироты;
- схема физически не может хранить аудио.
