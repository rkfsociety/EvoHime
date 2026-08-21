# 10-1 — CoreInfo и version negotiation

## Цель

Сделать совместимость Core и shell явной до начала рабочей сессии, сохранив
совместимость с текущими `Ready`/`Handshake` consumers.

## Что уже есть в checkout

- `Handshake` рекламирует protocol, session epoch и capabilities;
- Core сначала отправляет `AuthChallenge`, затем `Ready`;
- Rust и Electron согласуют одинаковый major, выбирают меньший minor и
  пересекают capabilities `replay`/`resync`;
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

1. Добавить сообщение `CoreInfo` и аддитивное поле `Ready.core_info` с
   bounded-полями:

   - `ProtocolVersion protocol`;
   - `core_version`, `build_revision`, `runtime_revision` — короткие
     redacted identity strings;
   - `capabilities` — capability tokens, участвующие в intersection;
   - `feature_flags` — только informational flags, не security authority;
   - `max_frame_bytes`, `max_replay_events`, `max_snapshot_bytes`.

   `core_instance_id` и `session_epoch` не дублируются в `CoreInfo`: для
   каждого envelope источником истины остаются существующие поля envelope.
   Старый consumer, который игнорирует `core_info`, продолжает работать по
   старому `Ready` contract.

2. Использовать ровно один алгоритм на Rust и Electron: major должен
   совпадать, minor становится `min(local, peer)`, capability set —
   пересечение после bounded validation. Capability, отсутствующая в
   intersection, не может быть отправлена в рабочей команде.

3. Вычислять effective limits как минимум локального и peer limit. Нулевые,
   противоречивые или превышающие hard limit значения — typed
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
- Electron: typed `CoreAvailability`/reason вместо неограниченного `reason`
  string, при этом старые connection states должны быть мигрированы
  совместимо;
- compatibility shell: отсутствие `core_info` трактуется как legacy peer,
  а не как повреждённый frame.

## Проверки

- same major/different minor и exact capability intersection;
- major mismatch, unknown required feature и invalid/zero/oversized limits;
- unavailable Core, auth rejection, stale session и reconnect;
- epoch/instance change сбрасывает cache, queue и sequence до resync;
- oversized frame/request отклоняется без изменения projection;
- один known-answer fixture для Rust, Electron и C# protocol/auth path;
- `npm run check:protocol`, targeted Rust desktop-ipc tests и Electron
  protocol/pipe-client tests.

## Готово, когда

Shell не отправляет рабочую команду до успешного auth + negotiation, каждый
отказ имеет typed availability code, effective limits одинаковы на Rust и
Electron, а старый compatibility consumer продолжает принимать `Ready`
без `core_info`.
