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
последняя миграция (v24) добавила `coordinator_child_checkpoint` и
`child_parent_sequences` для child workflows. Механика миграций — единственная
функция `migrate` (`lib.rs:2158`): цепочка блоков `if current < N { … PRAGMA
user_version = N; }` внутри одной транзакции, с бэкапом базы до применения и
восстановлением из него при сбое. Образцом для стора служат `memory_store.rs`
(tombstone, forget, expiry) и `context_ledger_store.rs`. Таблиц ambient нет.

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
  UNIQUE(episode_id, removed_at)
);
CREATE INDEX idx_ambient_utterances_episode ON ambient_utterances(episode_id, sequence);
CREATE INDEX idx_ambient_expiry ON ambient_utterances(expires_at);
CREATE INDEX idx_ambient_episode_expiry ON ambient_episodes(expires_at);
```

  Колонок для аудио нет **по конструкции**. Включённый `foreign_keys` не
  допускает осиротевших высказываний.
- Удаление выполняется транзакционно: сначала фиксируется metadata-only
  tombstone с количеством удаляемых высказываний, затем удаляются utterances и
  episode. `purge_expired` также создаёт bounded tombstone для удалённого
  эпизода; tombstones не содержат текст и сами имеют отдельный срок хранения.
  При частичном истечении utterances счётчики episode пересчитываются в той же
  транзакции.
- `crates/evohime-local-storage/src/ambient_store.rs`: вставка высказывания,
  открытие и закрытие эпизода, выборка эпизодов и высказываний с лимитами,
  дедупликация по `text_hash` в окне, удаление по эпизодам и по временному окну
  с tombstone, `purge_expired`.
- Retention: `EVOHIME_AMBIENT_RETENTION_DAYS` (по умолчанию 7, потолок 90) для
  текста, 30 дней для метаданных эпизодов; purge при старте Core и раз в час
  фоновой задачей. Образец — `spawn_receipt_retention` (`lib.rs:3698`):
  `tokio::spawn` с `loop { sleep(…); … }`, берущий `journal.clone()` и
  возвращающий `JoinHandle<()>`. `CancellationToken` в этих задачах сегодня
  **не используется** (вопреки ранней редакции плана); ambient-purge либо
  повторяет существующий паттерн, либо вводит отмену — но тогда как
  осознанное изменение, а не «как у остальных».
  Metadata-only tombstones удерживаются bounded-сроком (не дольше 30 дней) и
  также удаляются отдельным purge.
- `ambient-policy.json` в data dir: атомарная запись через временный файл и
  rename. Повреждённый файл читается как дефолтная политика **с включённой
  паузой** — fail-safe в пользу тишины.
- Удаление эпизода отклоняет memory-кандидатов с
  `provenance_source_id = episode_id` причиной `source_deleted` — тем же
  приёмом, что `forget_with_tombstone` в `memory_store.rs:831`.
- **Остаточное окно в бэкапах.** `create_backup` снимает копию базы перед
  миграцией, а `purge_expired_backups` (`backup.rs:245`) вычищает контейнеры по
  собственному сроку (в тестах — 7 суток). Удаление эпизода **не** достаёт
  текст из уже созданного контейнера: до истечения его срока транскрипт
  физически остаётся на диске. Ранняя редакция плана связывала ротацию бэкапов
  с `forget`, что неверно — это независимые механизмы. Окно называется явно в
  UI-тексте про удаление и в `docs/architecture.md`, а не заметается под
  формулировку «удалено безвозвратно».
- Ошибка вставки (диск заполнен, блокировка или другая ошибка SQLite) не
  ретраится листенером: Core возвращает `storage_failed`, не создаёт ложную
  запись и публикует `ambient.storage_error` в UI. Листенер помечает исходное
  высказывание как `dropped` и продолжает работу со следующим сегментом.
- **Ambient-события и durable journal.** События публикуются через
  `append_event(subject_id, event_type, payload)` в таблицу `events`, откуда их
  вычитывают клиенты (`push_journal_tail` в `pipe_server.rs`). Таблица
  **append-only и не имеет retention вообще** — ни purge, ни срока. Значит
  наивная публикация `ambient.transcript`/`ambient.state` оставляет вечный
  след: список `episode_id` с числом высказываний и полную хронологию того,
  когда пользователя слушали, — причём этот след переживает
  `DeleteAmbientTranscripts` и `forget_window`. Правило этапа: (1) в
  ambient-событиях нет ни текста, ни `text_hash`; (2) `ForgetAmbientWindow` и
  удаление эпизодов в той же транзакции удаляют из `events` строки с
  `event_type LIKE 'ambient.%'`, попадающие в окно или ссылающиеся на удалённый
  `episode_id`; (3) для `ambient.state` вводится собственный срок хранения,
  равный retention метаданных эпизода (30 дней), и он вычищается тем же
  purge-циклом. Без этого критерий «забыть последние N минут» не выполняется.
- `forget_window(minutes)` удаляет высказывания с `started_at` в замкнутом
  окне `[now - minutes, now]` и отклоняет производные кандидаты. Эпизод,
  начавшийся до окна, не удаляется целиком только из-за того, что пересекает
  его границу; после удаления счётчики обновляются, а пустой эпизод удаляется
  в той же транзакции.

## Файлы

- изменить: `crates/evohime-local-storage/src/lib.rs` (миграция v25,
  `SCHEMA_VERSION = 25`, экспорт модуля);
- создать: `crates/evohime-local-storage/src/ambient_store.rs`;
- изменить: `crates/evohime-core/src/lib.rs` (purge-задача).

## Проверки

- миграция с v24 на v25 (и upgrade с каждой поддерживаемой более ранней версии)
  не теряет данные и идемпотентна; при искусственном сбое миграции база
  восстанавливается из бэкапа, как в существующем тесте
  `restores_backup_when_migration_fails`;
- `PRAGMA table_info` для ambient-таблиц не содержит BLOB-колонок;
- retention удаляет ровно просроченное и оставляет tombstone;
- удаление эпизода атомарно сохраняет tombstone до каскадного удаления и не
  оставляет осиротевших кандидатов памяти или ссылок;
- `forget_window(minutes)` удаляет высказывания в окне и отклоняет производных
  кандидатов;
- после `forget_window` и после `DeleteAmbientTranscripts` в таблице `events`
  не остаётся `ambient.*`-строк, ссылающихся на удалённые эпизоды;
- `ambient.*`-события ни при каких входных данных не содержат `text` или
  `text_hash`;
- повреждённый `ambient-policy.json` даёт дефолт с включённой паузой.

## Критерии готовности

- транскрипты хранятся, читаются, истекают и удаляются под потолками;
- удаление не оставляет следа в durable event journal, а остаточное окно в
  backup-контейнерах названо пользователю честно;
- удаление источника не оставляет памяти-сироты;
- схема физически не может хранить аудио.
