# План 05.7 — Crash recovery

Статус: этап плана [05](05-0-model-request-provenance.md).

Цель этапа — честно закрывать committed dispatchable-запросы, оставшиеся без
terminal outcome после аварийного завершения: не удалять envelope, не
выдумывать успех и не выполнять слепой повторный dispatch.

## Зависимости

### Блокирующие

- [05.2](05-2-durable-storage.md) — `model_requests`, `status`,
  `dispatch_at` и `completed_at`;
- [05.3](05-3-request-integration.md) — единственная граница
  `commit -> dispatch` и fail-closed marker перед вызовом provider;
- [05.5](05-5-receipt-and-tool-linkage.md) — authoritative
  `model_responses`, `tool_intents` и точная request linkage;
- существующие recovery foundation и supervisor.

05.5 является блокирующей зависимостью: без `model_responses` нельзя отличить
зафиксированный `interrupted`/`failed` response от отсутствующего response, а
без `tool_intents` нельзя сохранить или диагностировать незавершённую связь с
tool effect. До 05.5 recovery не имеет безопасной деградации и не считается
реализуемым.

### Опциональные

- [05.8](05-8-redaction-and-retention.md) — операции, переводящие request в
  `redacted` или `retention_pruned`. До 05.8 эти состояния всё равно являются
  финальными и recovery их пропускает; после 05.8 пропуск также обязан
  сохранять typed retention-семантику и не пытаться реконструировать payload.

## Термины и область действия

**Committed dispatchable envelope** — строка `model_requests`, записанная
через `FullForDispatch`: `payload_mode = full`, полный payload прошёл
валидацию, а после 05.5 у запроса есть ровно один request receipt. Фикстуры и
legacy/migration-строки `payload_mode = hash_only` не являются dispatchable
запросами и не входят в recovery.

**Recovery candidate** — только строка, удовлетворяющая условию:

```sql
status = 'active' AND completed_at IS NULL
```

`redacted`, `retention_pruned` и любые другие terminal-статусы не являются
кандидатами. Recovery не переписывает их и не придумывает им
`completed_at`.

`dispatch_at` — durable marker, который 05.3 обязан записать и закоммитить
до вызова provider. Если marker не удалось сохранить, provider не вызывается.
Поэтому `dispatch_at IS NULL` после restart доказывает, что provider dispatch
не начинался. Наличие marker доказывает только возможную отправку, но не
доказывает ответ provider.

`finish_reason`, timeout и отсутствие локального response сами по себе не
доказывают успешное завершение. Timeout после `dispatch_at` всегда ведёт к
`unknown_outcome`, если нет authoritative response с другим состоянием.

## Правила классификации

Recovery читает request, `model_responses` и связанные `tool_intents` в одной
SQLite-транзакции и выбирает наиболее сильное доступное доказательство:

| Наблюдаемое состояние | Новый статус `model_requests` | Правило |
| --- | --- | --- |
| Валидный response с `status = interrupted`, включая partial output | `interrupted` | Authoritative response из 05.5 явно фиксирует оборванный stream. |
| Валидный response с `status = failed` | `failed` | Это уже зафиксированный provider outcome, а не придуманный recovery failure. |
| Валидный response с `status = complete` и `dispatch_at IS NOT NULL` | `completed` | Успех разрешён только по существующему authoritative complete response; recovery сам response не создаёт. |
| Response отсутствует, `dispatch_at IS NULL`, и нет противоречивой response/tool linkage | `interrupted` | Durable pre-dispatch marker доказывает отсутствие отправки. |
| Response отсутствует при `dispatch_at IS NOT NULL` | `unknown_outcome` | Provider мог завершить работу до краша, но результат не был сохранён. |
| Response имеет неполную/противоречивую linkage, неизвестный статус или `complete` при `dispatch_at IS NULL` | `unknown_outcome` | Recovery выбирает консервативную классификацию и пишет invariant diagnostic. |
| Есть pending tool effect без terminal receipt | статус model request определяется response/dispatch evidence | Это не превращает response в complete и не разрешает blind retry; tool effect остаётся `pending_recovery/unknown` по 05.5. |

`model_responses.status = interrupted` имеет приоритет над отсутствием
`dispatch_at`: partial response считается явным interrupted outcome даже при
неполном локальном marker. `finish_reason` только подтверждает уже валидный
status и не создаёт status самостоятельно.

Наличие `tool_intents` без валидного response не доказывает outcome модели.
Однако orphan или несовпадающие `origin_request_id`/
`origin_request_envelope_hash` — противоречивая evidence и дают
`unknown_outcome`, а не `interrupted`. Recovery не удаляет и не исправляет
такой intent.

## Механизм recovery

1. Core после открытия базы и завершения миграций запускает один recovery pass
   до снятия startup gate и до принятия нового model dispatch. Supervisor
   отвечает за restart Core, но не меняет `model_requests` самостоятельно.
2. Recovery создаёт `recovery_run_id`, пишет bounded structured event
   `model_request_recovery_started` и пакетно выбирает только committed
   `FullForDispatch` candidates по `status = 'active' AND completed_at IS NULL`.
   Hash-only строки получают отдельный `skipped_non_dispatchable` diagnostic и
   не классифицируются как interrupted/unknown.
3. Для каждого `request_id` открывается bounded `BEGIN IMMEDIATE`. Строка
   перечитывается внутри транзакции. Если другой поток уже сделал terminal
   переход, recovery делает no-op. Затем читаются ровно один допустимый
   `model_response` и все связанные `tool_intents`, проверяются FK,
   request/hash linkage, status enum и согласованность с `dispatch_at`.
4. При выбранном terminal outcome recovery обновляет только lifecycle-поля:

   ```sql
   UPDATE model_requests
      SET status = :terminal_status,
          completed_at = :completed_at
    WHERE request_id = :request_id
      AND status = 'active'
      AND completed_at IS NULL;
   ```

   `completed_at` берётся из валидного terminal `model_response.completed_at`,
   если он есть; иначе ставится текущая UTC-временная метка recovery. Для
   `unknown_outcome` это время reconcile, а не утверждение о времени
   фактического provider outcome.
5. В одной транзакции с переходом записывается bounded recovery audit event с
   `request_id`, прежним и новым статусом, reason/evidence kind,
   `dispatch_at`, `response_id`, числом intents и `recovery_run_id`. В event
   нельзя писать envelope, output, tool arguments, credentials или raw error.
   Отдельно пишутся `model_request_recovery_reconciled` и
   `model_request_recovery_failed`; aggregate `..._finished` содержит counts.
6. Recovery повторяет выборку до пустого результата, пока startup gate закрыт.
   После успешного прохода Core разрешает новый dispatch. Уже terminal rows
   при повторном запуске не изменяются.

Recovery не создаёт синтетический `model_response` для отсутствующего ответа.
Если для отдельного tool recovery потребуется `origin_kind = recovery`, такой
intent обязан использовать точные `origin_request_id` и
`origin_request_envelope_hash` и пройти весь policy/receipt path из 05.5;
простое закрытие model request такой intent не создаёт.

## Идемпотентность, crash recovery и неизменяемость

- Кандидат повторно проверяется внутри транзакции, а `UPDATE` использует
  compare-and-set по `status = active AND completed_at IS NULL`. Повторный
  запуск не переписывает terminal status или его `completed_at`.
- Recovery не меняет `envelope_blob`, `envelope_hash`, lineage, sources,
  block refs, block bytes, request receipt, `model_responses` или
  `tool_intents`. Изменяются только `model_requests.status` и
  `model_requests.completed_at`, плюс audit/diagnostic surface.
- Если процесс recovery падает до `COMMIT`, SQLite откатывает lifecycle и
  audit rows; следующий запуск повторяет кандидат. Recovery не вызывает
  provider и не выполняет blind retry.
- Если миграция/транзакция recovery не может быть записана после bounded retry,
  Core оставляет request active, пишет failure diagnostic и не снимает startup
  gate. Supervisor может перезапустить Core с обычным backoff; бесконечная
  серия одинаковых ошибок не должна превращаться в ложный terminal outcome.

## Redaction, retention и hash-only

- `payload_mode = hash_only` пропускается: 05.3 запрещает dispatch для такой
  строки, поэтому recovery не вправе выдумать её outcome. Если у hash-only
  строки обнаружен ненулевой `dispatch_at`, это invariant violation; строка
  остаётся active, outcome не становится success.
- `redacted` и `retention_pruned` пропускаются независимо от наличия
  `envelope_blob`/block bytes. Recovery не пытается восстановить текст, не
  меняет retention status и не устанавливает `completed_at` задним числом.
- Для full envelope recovery использует только сохранённые request/response/
  intent rows. Workspace, текущая модель, последний chat message и повторная
  сборка контекста не являются источниками evidence.

## Retry lineage

`interrupted` и `unknown_outcome` остаются полноценными terminal request
outcomes: их immutable `envelope_hash` может быть указан как
`previous_request_hash` у следующей явно созданной attempt. Это не означает,
что request был успешен, и не разрешает автоматический повтор. Для
`redacted`/`retention_pruned` hash также может сохранять lineage-связь, но
новый dispatch невозможен без доступного полного payload и policy,
разрешающей такую попытку.

## Тесты

### Unit

- `model_responses.status = interrupted` с partial output даёт
  `model_requests.status = interrupted`;
- валидные `complete`/`failed` responses зеркалятся только при корректной
  linkage и допустимом `dispatch_at`;
- отсутствие response при `dispatch_at IS NULL` даёт `interrupted`;
- отсутствие response после `dispatch_at`, timeout и неполная linkage дают
  `unknown_outcome`;
- `complete` при `dispatch_at IS NULL` никогда не даёт success;
- finish reason без authoritative response не классифицирует request;
- terminal transition всегда выставляет `completed_at`, а redacted/retention
  rows не получают его от recovery;
- hash-only request пропускается, а ненулевой marker на hash-only фиксируется
  как invariant violation;
- orphan/mismatched `tool_intent` не исправляется и не становится поводом для
  blind retry;
- envelope и immutable linkage не изменяются.

### Integration

1. **Crash до dispatch:** full envelope committed, `dispatch_at IS NULL`,
   response отсутствует; после restart request сохранён и получает
   `interrupted` с ненулевым `completed_at`, provider не вызывается.
2. **Crash после marker:** `dispatch_at IS NOT NULL`, response отсутствует;
   recovery выставляет `unknown_outcome`, а не success/interrupted.
3. **Partial response:** crash/cancellation оставляет authoritative
   `model_responses.status = interrupted`; request получает interrupted,
   partial output не считается complete.
4. **Existing terminal response:** complete/failed response и request linkage
   корректно закрывают active request; recovery не создаёт дубль response.
5. **Tool linkage:** pending tool effect и recovery/system intent сохраняют
   точный request id/hash; recovery не ретраит tool и не теряет intent.
6. **Idempotence:** повторный startup pass, конкурентный reconcile и уже
   terminal request не переписывают status/`completed_at`.
7. **Redaction/retention:** такие rows пропускаются, envelope/receipt/hash
   остаются неизменными, payload не реконструируется.
8. **Recovery crash:** искусственный сбой до commit оставляет request active,
   не оставляет частичный audit transition и успешно повторяется после
   supervisor restart.
9. **Audit:** один recovery run имеет start/reconciled/finished либо
   failed events с bounded metadata и без секретов или model-visible текста.

## Критерии готовности

1. После аварийного завершения ни один committed `FullForDispatch` envelope
   не исчезает и не переписывается.
2. Recovery запускается Core при старте до нового model dispatch и сканирует
   только `status = 'active' AND completed_at IS NULL`.
3. `interrupted` назначается только по authoritative interrupted response или
   доказанному отсутствию provider dispatch через committed null marker;
   `unknown_outcome` назначается во всех остальных неопределённых случаях.
4. Ни один request без authoritative complete response не получает
   `completed`; recovery никогда не создаёт synthetic success.
5. Каждый recovery terminal transition (`completed`, `failed`, `interrupted`,
   `unknown_outcome`) устанавливает ненулевой `completed_at`.
6. Повторный и конкурентный recovery идемпотентны, не переписывают уже
   terminal statuses и безопасны при crash до commit.
7. `model_responses` и `tool_intents` сохраняют exact request linkage;
   partial response и pending tool effect не теряются и не запускают blind
   retry.
8. `redacted`, `retention_pruned` и hash-only записи не реконструируются и не
   получают ложный terminal outcome от recovery.
9. Факт запуска, результата и ошибки recovery наблюдаем в bounded structured
   audit/log events без payload, секретов и raw error text.
