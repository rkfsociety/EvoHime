# 10-1 — CoreInfo и version negotiation

## Цель

Сделать совместимость Core и shell явной до начала рабочей сессии, сохранив
совместимость с текущими `Ready`/`Handshake` consumers.

## Что уже есть в checkout

- `Handshake` рекламирует protocol, session epoch и capabilities; для
  аддитивного offer уже есть отдельный `ProtocolOffer`;
- Core сначала отправляет `AuthChallenge`, затем `Ready`; сейчас `Ready`
  занимает поля 1 (`protocol`) и 2 (`core_version`);
- `negotiate_protocol` в `crates/desktop-ipc/src/lib.rs` согласует одинаковый
  major, выбирает меньший minor и пересекает capabilities `replay`/`resync`;
  Electron повторяет тот же алгоритм в `protocol-version.ts`;
- bounded limits уже заданы константами: `MAX_FRAME_BYTES` = 4 MiB,
  `DEFAULT_RESYNC_MAX_EVENTS` = 512, `MAX_REPLAY_EVENTS` = 512,
  `MAX_RESYNC_SNAPSHOT_BYTES` = `MAX_FRAME_BYTES - 1024`,
  `MAX_CAPABILITIES` = 64, `MAX_CAPABILITY_NAME_BYTES` = 64;
- `CorePipeClient` уже останавливает работу при major mismatch, сбрасывает
  sequence и очередь при смене `core_instance_id/session_epoch`, а затем
  делает bounded resync.

Этап добавляет данные, которых сейчас нет, и не переписывает уже работающий
auth/reconnect flow.

## Зависимости

### Блокирующие

- контракты 08-3/08-4 после их принятия для replay, gap и Core generation
  semantics;
- контракты 09-1/09-4 после их принятия для capability/policy names;
- текущие generated Rust, Electron и C# compatibility fixtures.

### Опциональные

- provider features не блокируют transport negotiation: до завершения
  adapter boundary negotiation знает только transport capabilities.

## Контракт

1. Добавить сообщение `CoreInfo` и аддитивное поле `Ready.core_info = 3`
   (поля 1 и 2 заняты, field numbers не переиспользуются) с bounded-полями:

   - `ProtocolVersion protocol`;
   - `core_version`, `build_revision`, `runtime_revision` — короткие
     redacted identity strings;
   - `capabilities` — capability tokens, участвующие в intersection;
   - `feature_flags` — только informational flags, не security authority;
   - `max_frame_bytes`, `max_replay_events`, `max_snapshot_bytes`.

   Каждая строка проверяется тем же bounded-правилом, что и capability names
   (`MAX_CAPABILITY_NAME_BYTES`, непустая, без ASCII control chars), а число
   capabilities и flags не превышает `MAX_CAPABILITIES`. Отдельного
   negotiation-сообщения не вводится: `ProtocolOffer` и `Handshake` остаются
   единственным offer-путём, `CoreInfo` только описывает peer.

   `core_instance_id` и `session_epoch` не дублируются в `CoreInfo`: для
   каждого envelope источником истины остаются существующие поля envelope.
   Старый consumer, который игнорирует `core_info`, продолжает работать по
   старому `Ready` contract.

2. Использовать ровно один алгоритм на Rust и Electron — уже существующий
   `negotiate_protocol`/`protocol-version.ts`, а не новый: major должен
   совпадать, minor становится `min(local, peer)`, capability set —
   пересечение после bounded validation. Capability, отсутствующая в
   intersection, не может быть отправлена в рабочей команде.

3. Вычислять effective limits как `min(local, peer)`; результат никогда не
   поднимается выше локальной hard-константы, даже если peer объявил больше.
   Нулевые, противоречивые или превышающие hard limit значения — typed
   `unsupported`, а frame/request, превышающий effective limit, отклоняется
   до dispatch. Для `ResyncRequest` сохранить правило: `max_events=0` значит
   bounded default, значение выше лимита отвергается.

4. Зафиксировать typed availability mapping в main adapter:

   - pipe не найден, connect/handshake timeout — `unavailable`;
   - major/limit/required-feature mismatch — `unsupported`;
   - malformed/неполный `Ready`, неизвестное состояние до завершения
     handshake — `unknown`;
   - смена instance/epoch или ответ с прошлой generation — `stale_session`.

   Эти значения не должны маскироваться обычной строкой transport error и не
   должны смешиваться с provider `unavailable`.

5. После нового `core_instance_id/session_epoch` capability cache и
   negotiated provider view сбрасываются. Команды, стоявшие в очереди старой
   generation, не отправляются; после `Ready` выполняется bounded replay или
   snapshot согласно `ReplayGap`/`ResyncRequest`.

## Изменения по слоям

- proto: `CoreInfo` и `Ready.core_info`, без изменения существующих field
  numbers и без protocol major bump;
- Rust: общий validator/fixture для CoreInfo и effective limits;
- Electron: аддитивные typed `CoreAvailability`/`reason class` в `ShellState`
  (`src/shared/api.ts`); существующий union `ConnectionState`
  (`starting`…`fatal`) и bounded `reason` string сохраняются, чтобы renderer
  и его тесты не ломались, а typed code становится вторым полем, а не
  заменой;
- compatibility shell: отсутствие `core_info` трактуется как legacy peer,
  а не как повреждённый frame.

## Проверки

- same major/different minor и exact capability intersection;
- major mismatch, unknown required feature и invalid/zero/oversized limits;
- unavailable Core, auth rejection, stale session и reconnect;
- epoch/instance change сбрасывает cache, queue и sequence до resync;
- oversized frame/request отклоняется без изменения projection;
- `Ready` без `core_info` принимается как legacy peer, а не как ошибка;
- один known-answer fixture для Rust, Electron и C# protocol/auth path;
- `npm run check:protocol`, targeted Rust desktop-ipc tests и Electron
  protocol/pipe-client tests.

## Готово, когда

Shell не отправляет рабочую команду до успешного auth + negotiation, каждый
отказ имеет typed availability code, effective limits одинаковы на Rust и
Electron, а старый compatibility consumer продолжает принимать `Ready`
без `core_info`.
