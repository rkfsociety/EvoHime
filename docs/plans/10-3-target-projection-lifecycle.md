# 10-3 — Target scope и stale projection lifecycle

## Цель

Безопасно переключать workspace, provider и backend, не смешивая состояние
старого target с новым.

## Изменения

1. Ввести typed `WorkspaceTarget` и target identity, связанную с session,
   capability snapshot и query scope.
2. При смене target атомарно закрывать старый query scope, сбрасывать
   capability cache и отменять неподходящие in-flight requests.
3. Прекратить отправку команд в старый Core/provider/backend после смены
   target; stale response помечать и не применять к новой projection.
4. Ввести typed `TransportError`/`StaleSession` с причиной, target и текущей
   revision без raw secret или unbounded payload.
5. После Core restart выполнять bounded replay/snapshot и восстанавливать
   только projection текущей session/target.

## Проверки

- смена workspace/provider во время запроса;
- stale response после target switch;
- reconnect и Core restart с очисткой projection;
- provider fallback без команды старому target;
- query/path/secret scope isolation между targets.

## Готово, когда

Каждый видимый результат можно связать с активным target и Core revision, а
устаревшие данные автоматически исключаются из UI и не вызывают side effect.
