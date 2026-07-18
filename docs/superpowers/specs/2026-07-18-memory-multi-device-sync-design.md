# Multi-device memory sync design

> Дата: 2026-07-18  
> Статус: approved design  
> Roadmap: `7.50`

## Цель

Синхронизировать structured memory между несколькими доверенными экземплярами EvoHime одного оператора, сохраняя локальную автономность, redaction, provenance и безопасное поведение при конфликте.

Синхронизация не должна превращать memory в общий безымянный cloud-кэш: каждый экземпляр имеет стабильную replica identity, сервер хранит журнал изменений, а локальный агент продолжает работать при недоступном sync-сервисе.

## Границы первой реализации

В scope входят:

- один оператор и одна sync-группа;
- два и более устройства/инсталляции;
- memory items, их статусы, content, content_json, scope, scope_key, confidence, importance, pinned и validity fields;
- pull/push по cursor с идемпотентным применением;
- offline-first работа и повторная отправка после восстановления сети;
- конфликты, soft-delete и ручное разрешение через существующий MemoryPanel.

Не входят:

- совместное редактирование несколькими пользователями;
- синхронизация session memory и chat history;
- синхронизация embeddings как истины;
- автоматическое продвижение конфликтного или импортированного item в `active`;
- end-to-end encryption от sync-сервера в первой версии;
- peer-to-peer discovery и realtime transport.

## Модель идентичности

Локальный `memory_items.id` остаётся UUID строки, но для sync добавляются независимые поля:

| Поле | Назначение |
| --- | --- |
| `sync_group_id` | Логическая группа памяти оператора |
| `replica_id` | Стабильный UUID установки, создаётся при первом запуске и хранится локально |
| `sync_id` | Стабильный UUID memory item, общий для всех реплик |
| `revision` | Монотонная ревизия item на origin replica |
| `updated_by_replica` | Какая реплика породила последнюю ревизию |
| `deleted_at` | Tombstone вместо физического удаления |
| `last_synced_cursor` | Локальный cursor pull для каждой sync-группы |

Текущий UUID нельзя использовать как sync identity: импорт и существующие записи уже создаются независимо на разных устройствах. При включении sync существующие items получают `sync_id` один раз; последующие export/import операции не переопределяют его без явного link action.

## Хранилище и журнал

Добавляются таблицы:

```text
sync_devices(
  sync_group_id, replica_id, device_name, public_key?, last_seen_at,
  revoked_at, created_at
)

memory_sync_state(
  sync_group_id, replica_id, pull_cursor, push_cursor, last_error, updated_at
)

memory_change_log(
  sequence bigint generated always as identity,
  sync_group_id, sync_id, origin_replica_id, revision,
  operation, payload jsonb, occurred_at
)
```

`memory_change_log` append-only. Payload содержит portable fields и provenance, но не embedding. Для удаления публикуется tombstone с `sync_id`, `revision`, origin и временем удаления. Retention журнала — не меньше максимального offline window плюс запас; после compaction сервер выдаёт snapshot cursor, а клиент делает полный scoped snapshot.

Индексы обязательны по `(sync_group_id, sequence)`, `(sync_group_id, sync_id, revision)` и активным tombstones. Запись item и change-log должна выполняться одной транзакцией.

## Протокол

### Pull

```http
GET /api/memory/sync/changes?group_id=<id>&after=<cursor>&limit=500
Authorization: Bearer ...
```

Ответ:

```json
{
  "cursor": "opaque-server-cursor",
  "has_more": true,
  "changes": [
    {
      "sequence": 1842,
      "sync_id": "uuid",
      "origin_replica_id": "uuid",
      "revision": 3,
      "operation": "upsert",
      "item": {"scope":"workspace", "scope_key":"repo", "kind":"fact", "content":"..."}
    }
  ]
}
```

### Push

```http
POST /api/memory/sync/changes
Authorization: Bearer ...
Idempotency-Key: <replica-id>:<batch-id>
```

Клиент отправляет batch до 100 изменений с `sync_id`, origin revision и payload. Сервер отвечает применёнными изменениями, duplicate acknowledgements и конфликтами. Повтор той же пары `(origin_replica_id, sync_id, revision)` безопасен и не создаёт вторую запись.

### Snapshot

```http
GET /api/memory/sync/snapshot?group_id=<id>
```

Используется только при просроченном cursor или первом подключении. Snapshot переносит portable memory fields и tombstones, после чего клиент продолжает pull с возвращённого cursor.

## Правила merge

1. Одинаковая `(sync_id, revision)` — duplicate acknowledgement.
2. Более новая revision того же origin — заменить старую, если локальная запись не имеет более новой несинхронизированной revision.
3. Изменения от разных origin для одного `sync_id` — не выбирать победителя по wall-clock. Создать `conflict`, сохранить обе версии и исключить item из retrieval.
4. Удаление — обычная ревизия tombstone; оно не должно проигрывать старой upsert-записи из-за часов устройства.
5. Если переносимый item не имеет `sync_id` или принадлежит другой группе, принять его как новый candidate через существующий redaction/normalization/dedupe/conflict flow.
6. `embedding` и `embedding_version` не синхронизируются: после принятия item локальный worker ставит embedding на пересчёт.
7. Conflict resolution создаёт новую ревизию с выбранным содержимым и помечает проигравшую версию superseded, используя текущий atomic resolve flow.

Никакого last-write-wins по timestamp: часы устройств ненадёжны, а тихая потеря пользовательского memory противоречит safety-политике EvoHime.

## Безопасность и границы

- Sync отключён по умолчанию и не меняет текущий local-only режим.
- В первой реализации sync endpoint разрешён только после явного pairing-кода или device token; bearer API token не является автоматически pairing authority.
- Сервер проверяет `sync_group_id`, replica registration, размер batch, размер payload и scopes.
- Redaction выполняется до постановки изменения в change-log; сервер повторяет validation на push.
- `workspace` и `project` items принимаются только при совпадении workspace identity, иначе становятся candidate с quarantine reason.
- Revoked replica больше не может push, но её ранее опубликованные изменения остаются auditable.
- Change-log и snapshot не содержат source session/task IDs, private embeddings или секретов.
- UI показывает last sync, pending changes, cursor error и число конфликтов; синхронизация не блокирует чат и tools.

## Offline и восстановление

Локальная запись сначала коммитится в `memory_items` и outbox/change-log, затем отправляется. Ошибка сети оставляет item доступным локально. Retry использует exponential backoff и idempotency key. После `cursor_expired` клиент загружает snapshot во временные таблицы, применяет merge, затем атомарно заменяет cursor.

Если push вернул conflict, локальная версия остаётся candidate/conflict и видна в MemoryPanel. Автоматический retry не должен повторно менять статус или создавать дубликаты.

## План реализации

1. `7.50a`: миграции identity/tombstone, replica settings и append-only outbox.
2. `7.50b`: storage transactions, cursor log, idempotent push/pull и snapshot endpoint.
3. `7.50c`: pairing/device auth и group/workspace validation.
4. `7.50d`: background sync worker с backoff, metrics и recovery after restart.
5. `7.50e`: MemoryPanel sync status, conflict links и manual retry/resync.
6. После стабилизации: optional encrypted transport, retention/compaction и multi-user ACL как отдельная roadmap-ветка.

## Acceptance criteria

- Две реплики без сети создают одинаково изменённый item и после reconnect не теряют ни одну версию.
- Повтор push одного batch идемпотентен.
- Удалённый item не воскресает из старого cursor.
- Просроченный cursor восстанавливается через snapshot.
- Secrets не попадают в change-log.
- Embeddings пересчитываются локально и не ломают retrieval при несовпадении моделей.
- Conflict виден в существующем UI и разрешается явным действием оператора.
- При выключенном sync текущий single-device путь и pack export/import не меняются.
