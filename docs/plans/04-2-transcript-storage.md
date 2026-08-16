# Этап 04.2: Хранение транскриптов и retention

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: этап 04.1 — лимиты, состояния и схема политики приходят оттуда.

Разблокирует: 04.3–04.7.

## Что этап отдаёт наружу

Core-owned хранилище эпизодов и высказываний с retention, удалением и
tombstone, плюс персистентная политика ambient.

## Что уже есть в коде

`SCHEMA_VERSION = 20` в `crates/evohime-local-storage/src/lib.rs`; последняя
миграция создаёт `model_context_limits`. Образцом служат `memory_store.rs`
(tombstone, forget, expiry) и `context_ledger_store.rs`. Таблиц ambient нет.

## Содержание

- Миграция **v21**:

```sql
CREATE TABLE ambient_episodes (
  episode_id TEXT PRIMARY KEY NOT NULL,
  started_at TEXT NOT NULL, ended_at TEXT,
  utterance_count INTEGER NOT NULL, speech_ms INTEGER NOT NULL,
  engine_version TEXT NOT NULL, model_id TEXT NOT NULL,
  extraction_state TEXT NOT NULL,      -- skipped | pending | done
  expires_at TEXT NOT NULL
);
CREATE TABLE ambient_utterances (
  utterance_id TEXT PRIMARY KEY NOT NULL,
  episode_id TEXT NOT NULL, sequence INTEGER NOT NULL,
  started_at TEXT NOT NULL, duration_ms INTEGER NOT NULL,
  text TEXT NOT NULL, text_hash TEXT NOT NULL,
  language TEXT NOT NULL, avg_logprob REAL NOT NULL,
  speaker TEXT NOT NULL,               -- v1 всегда 'unverified'
  redacted INTEGER NOT NULL DEFAULT 0,
  expires_at TEXT NOT NULL
);
CREATE TABLE ambient_tombstones (
  tombstone_id TEXT PRIMARY KEY NOT NULL, episode_id TEXT NOT NULL,
  removed_at TEXT NOT NULL, reason TEXT NOT NULL, utterance_count INTEGER NOT NULL
);
CREATE INDEX idx_ambient_utterances_episode ON ambient_utterances(episode_id, sequence);
CREATE INDEX idx_ambient_expiry ON ambient_utterances(expires_at);
```

  Колонок для аудио нет **по конструкции**.
- `crates/evohime-local-storage/src/ambient_store.rs`: вставка высказывания,
  открытие и закрытие эпизода, выборка эпизодов и высказываний с лимитами,
  дедупликация по `text_hash` в окне, удаление по эпизодам и по временному окну
  с tombstone, `purge_expired`.
- Retention: `EVOHIME_AMBIENT_RETENTION_DAYS` (по умолчанию 7, потолок 90) для
  текста, 30 дней для метаданных эпизодов; purge при старте Core и раз в час
  фоновой задачей через `tokio::spawn` + `CancellationToken`, как остальные.
- `ambient-policy.json` в data dir: атомарная запись через временный файл и
  rename. Повреждённый файл читается как дефолтная политика **с включённой
  паузой** — fail-safe в пользу тишины.
- Удаление эпизода отклоняет memory-кандидатов с
  `provenance_source_id = episode_id` причиной `source_deleted` и вращает
  backup-контейнеры старше 7 дней — тем же приёмом, что `forget` в Memory
  Extraction.
- Ошибка вставки (диск заполнен, блокировка или другая ошибка SQLite) не
  ретраится листенером: Core возвращает `storage_failed`, не создаёт ложную
  запись и публикует `ambient.storage_error` в UI. Листенер помечает исходное
  высказывание как `dropped` и продолжает работу со следующим сегментом.
- `forget_window(minutes)` удаляет высказывания с `started_at` в замкнутом
  окне `[now - minutes, now]` и отклоняет производные кандидаты. Эпизод,
  начавшийся до окна, не удаляется целиком только из-за того, что пересекает
  его границу.

## Файлы

- изменить: `crates/evohime-local-storage/src/lib.rs` (миграция v21,
  `SCHEMA_VERSION = 21`, экспорт модуля);
- создать: `crates/evohime-local-storage/src/ambient_store.rs`;
- изменить: `crates/evohime-core/src/lib.rs` (purge-задача).

## Проверки

- миграция с v20 на v21 на существующей БД не теряет данных и идемпотентна;
- `PRAGMA table_info` для ambient-таблиц не содержит BLOB-колонок;
- retention удаляет ровно просроченное и оставляет tombstone;
- `forget_window(minutes)` удаляет высказывания в окне и отклоняет производных
  кандидатов;
- повреждённый `ambient-policy.json` даёт дефолт с включённой паузой.

## Критерии готовности

- транскрипты хранятся, читаются, истекают и удаляются под потолками;
- удаление источника не оставляет памяти-сироты;
- схема физически не может хранить аудио.
