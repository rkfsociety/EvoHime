# 10-3 — Target scope и stale projection lifecycle

## Цель

Безопасно переключать workspace, route/provider hint и backend, не смешивая
состояние старого target с новой projection и не повторяя внешний эффект
после гонки или restart.

## Что уже есть в checkout

- команды workspace/task уже несут `workspace_path`, который Core обязан
  валидировать; канонический производный scope уже вычисляет
  `workspace_scope_id` в `crates/evohime-core/src/task_memory.rs`
  (нормализует регистр и завершающий разделитель);
- `core_instance_id/session_epoch/sequence_id` позволяют отличать Core
  generation и journal revision;
- Electron `CorePipeClient` при epoch change сбрасывает sequence и queued
  commands, затем запрашивает bounded resync;
- model gateway уже имеет immutable route snapshot и per-run health overlay,
  но общего target generation для UI projection пока нет.

## Зависимости

### Блокирующие

- 10-1 для Core generation, effective limits и typed stale-session state;
- 10-2 для adapter session, route snapshot, scopes и cancellation;
- контракты 08-3/09-2 после их принятия для replay/projection и Core policy
  checks.

### Опциональные

- существующие workflow/child projections могут использовать тот же target
  metadata после адаптации; их наличие не блокирует базовый lifecycle;
- provider hot route selection. Credential change остаётся supervisor/Core
  restart и не требует hot-swap реализации.

## Target contract

1. Ввести Core-owned `ActiveTarget`:

   - `target_id` — stable bounded hash, не содержащий path или secret;
   - `target_generation` — monotonic counter внутри Core generation;
   - canonical `workspace_scope` — значение существующего
     `workspace_scope_id`, а не новый параллельный hash;
   - selected route/provider id и backend/adapter id;
   - `core_instance_id` и `session_epoch`.

   Raw workspace path, API key и opaque secret values в target identity,
   projection и error не попадают.

2. Для target-bound command/request Core фиксирует target snapshot и проверяет
   его в двух местах: непосредственно перед dispatch/effect и перед
   применением результата к durable/UI projection. Payload результата несёт
   только bounded target metadata (`target_id`, generation, Core generation,
   sequence).

3. Переключение выполняется как одна Core-owned transition:

   `Active → Switching(old generation) → Active(new generation)`.

   Внутри transition Core под lock помечает старый target stale, увеличивает
   generation, отменяет queued/not-started requests, закрывает старые query
   scopes, сбрасывает capability/adapter cache и публикует bounded
   `target.changed`. Новый snapshot строится только после этого commit.

4. Для уже начатого внешнего эффекта нельзя обещать физический rollback.
   После switch его ответ помечается `stale_session`/`unknown_outcome`, не
   применяется к новому target и не ретраится автоматически. Неотправленная
   команда вообще не dispatchится. Это заменяет небезопасное обещание
   «отменить любой in-flight эффект».

5. Fallback и retry используют только старый immutable snapshot, пока target
   остаётся active. После switch старый snapshot закрыт: fallback в другой
   workspace/provider/backend запрещён. При provider credential change новый
   `core_instance_id/session_epoch` считается новым Core generation, а не
   продолжением старой target projection.

6. Replay после Core restart сначала проверяет `(core_instance_id,
   session_epoch, target_id, target_generation)`. Старые события можно
   использовать только как bounded diagnostic; projection новой session
   восстанавливается через текущий snapshot/replay и не принимает старый
   side effect.

7. Ввести bounded typed `TransportError`/`StaleSession` с code, request id,
   target id, Core generation, target generation, current sequence и reason
   class. Raw payload, secret, prompt, path и unbounded provider error туда не
   входят.

## Проверки

- смена workspace/route во время queued, running и completed-before-apply
  operation;
- stale response после target switch не меняет UI projection и не вызывает
  новый side effect;
- Core restart очищает старый projection, принимает только текущий epoch и
  делает bounded replay/snapshot;
- provider fallback после switch не dispatchится старому target;
- одинаковые workspace scopes не получают доступ к чужим path/secret grants;
- внешний эффект, начатый до switch, получает `unknown_outcome` без blind
  retry;
- target IDs/errors/events bounded и не содержат raw secret/path/prompt;
- два разных написания одного пути (регистр, завершающий разделитель) дают
  один и тот же `workspace_scope` и не порождают лишний target switch.

## Готово, когда

Каждый видимый target-bound результат связан с active target и Core revision,
устаревшие ответы автоматически исключаются, queued old commands не уходят,
а уже начатые внешние эффекты не повторяются вслепую и явно остаются
`unknown_outcome`, если их результат нельзя подтвердить.
