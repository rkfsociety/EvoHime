# 10-1 — CoreInfo и version negotiation

## Цель

Сделать совместимость Core и shell явной до начала рабочей сессии.

## Изменения

1. Ввести bounded `CoreInfo` с protocol major/minor, build/runtime revision,
   core instance, capabilities, feature flags и limits.
2. Добавить typed states `unavailable`, `unsupported`, `unknown` и
   `stale_session`; не маскировать их как обычную ошибку transport.
3. Проверять major compatibility, supported feature и frame/operation limits
   до отправки рабочих команд.
4. Связать `CoreInfo` с session epoch, event replay и current Core revision;
   после restart capability cache сбрасывать.
5. Сохранить additive protocol evolution и compatibility tests для текущих
   C#/WinUI transitional consumers.

## Проверки

- major mismatch, minor feature negotiation и unsupported feature;
- unavailable Core, stale session и reconnect;
- limits mismatch и oversized frame rejection;
- protocol known-answer fixtures для Rust и Electron.

## Готово, когда

Shell не начинает рабочую сессию с несовместимым Core и показывает typed
состояние, которое позволяет безопасно reconnect или завершить попытку.
