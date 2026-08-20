# Этап 04.1: Контракт ambient и приватность

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: нет. Этап пишет side-effect-free контракт и capability поверх уже
существующих `crates/permissions` и `permission_rules`.

Разблокирует: все остальные этапы плана 04.

## Что этап отдаёт наружу

Единый bounded контракт ambient-слушания: состояния, лимиты, схема политики,
capability микрофона и правила логирования.

## Что уже есть в коде

Ambient-контракт и capability ещё не реализованы: в текущем checkout нет
аудио-крейта, ambient-веток в permissions и ambient-раздела в канонической
архитектурной документации. Существующие `Permission` и `permission_rules`
являются точками расширения, но их фактический API нужно сверить перед
изменением.
Прототип на Web Speech API удалён вместе с веб-фронтендом и к native-архитектуре
неприменим.

## Содержание

- Новый крейт `crates/evohime-listener-contract` в стиле
  `evohime-supervisor::schedule_contract`: без файловой системы, часов и
  процессов. В нём:
  - `ListeningState`: `Stopped`, `Starting`, `Listening`, `PausedByUser`,
    `PausedByPolicy`, `DeviceConflict`, `DeviceDisconnected`,
    `EngineUnavailable`, `Denied` и
    допустимые переходы;
  - `AmbientLimits`: кадр 30 мс, pre-roll 300 мс, hangover 700 мс, минимум
    400 мс, потолок высказывания 20 с, эпизод 10 мин, окно дедупликации 60 с;
  - `AmbientPolicy` v1 (пауза, тихие часы, чёрные списки процессов и заголовков
    окон, retention) с валидацией и потолками длины. В тихие часы поток
    захвата полностью закрыт: высказывания не распознаются и не сохраняются;
  - `ProactivityBudget` — неизменяемый снимок лимитов (3/час, 10/сутки,
    10 минут между предложениями) по образцу `run_policy`. Текущие счётчики и
    время последнего предложения не входят в snapshot и живут в Core-состоянии
    `ambient_proactivity_state`;
  - закрытый набор кодов ошибок.
- `Permission::MicrophoneListen` (serde `microphone_listen`) с дефолтом `Deny`;
  правило в `permission_rules.rs` и в `permissions.json.example`. Capability
  проверяется Core перед выдачей listener-разрешения, а не только в UI.
- Правила логирования: закрытый allow-list полей для `listener.jsonl` и для
  событий `ambient.*`. Текст, хеш текста, имя процесса и заголовок окна в логи
  не попадают никогда — короткую фразу по хешу перебирают за секунды, поэтому
  хеш приравнивается к содержимому.
- Раздел «Ambient listening» в `docs/architecture.md` как канонический контракт.

## Файлы

- создать: `crates/evohime-listener-contract/{Cargo.toml,src/lib.rs}`;
- изменить: `Cargo.toml` (workspace members), `crates/permissions/src/lib.rs`,
  `crates/evohime-core/src/permission_rules.rs`, `permissions.json.example`,
  `docs/architecture.md`.

## Проверки

- перебор всех переходов `ListeningState`: недопустимый переход отвергается;
- политика с чрезмерным regex, слишком длинным списком или retention > 90 дней
  отвергается до применения;
- `MicrophoneListen` по умолчанию `Deny` в чистом профиле;
- logger allow-list: попытка записать произвольный текст в ambient-лог падает
  в тесте.

## Критерии готовности

- контракт не имеет побочных эффектов и покрыт негативными тестами;
- capability микрофона существует, по умолчанию запрещена и видна в policy
  snapshot;
- ни одно поле контракта не позволяет протащить в лог содержимое речи.
