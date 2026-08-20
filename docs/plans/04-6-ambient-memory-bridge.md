# Этап 04.6: Мост ambient в память

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: этап 04.2 — источник provenance; этап 04.4 — сами транскрипты;
этап 04.5 — очередь подтверждения и признак источника видны пользователю.

Разблокирует: 04.7 — предложения опираются на извлечённых кандидатов.

## Что этап отдаёт наружу

Ambient как новый источник кандидатов памяти со строго более жёсткой policy,
не меняя инвариант «Core — единственный владелец, всё есть candidate».

## Что уже есть в коде

`memory_extraction.rs` реализован: `SourceTrust` из четырёх значений
(`:182`), `evaluate()` (`:997`) с единственным путём до `AutoConfirm` (явный
триггер, low risk, `SourceTrust::User`), `pending_confirmation` по умолчанию,
отказ секретам, конфликт по `kind + canonical_subject + scope`, tombstone на
`forget`. Ambient-источника и его policy нет; `run_memory_extraction`
(`lib.rs:4381`) жёстко привязана к паре (user_prompt, assistant_reply) одного
завершённого хода и другого входа не имеет.

Сверено дополнительно, потому что без этого этап не заработает:

- **`check_can_extract` (`memory_extraction.rs:1397`) отсечёт ambient раньше,
  чем дело дойдёт до `evaluate`**: при `mode == Strict && trigger.is_none()`
  она возвращает `Throttled { NoExplicitTrigger }`, а `Strict` — режим по
  умолчанию (`memory_extraction_mode()`, `lib.rs:9981`). Ambient по построению
  без триггера, поэтому общий гейт запуска для него неприменим как есть;
- `ThrottleReason` (`:460`) содержит `TurnLimit`, `HourlyLimit`, `TokenBudget`,
  `CircuitOpen`, `ModeDisabled`, `NoExplicitTrigger` — варианта под
  ambient-бюджеты нет;
- `PolicyReason` (`:918`) имеет только `as_str`, без обратного разбора; в
  renderer коды причин сегодня не отображаются, отдельной таблицы строк для
  них заводить не нужно;
- `user_asserted` для обычного пути вычисляется как `trigger.is_some()`
  (`lib.rs:4481`), поэтому для ambient он и так окажется `false`;
- `memory_provenance_source_id` (`lib.rs:10000`) берёт значение только из
  `RawEvidenceLocator`: `message_id` → `tool_call_id` → `task_id` →
  `file_path`. Поля под эпизод там нет;
- при ручной правке записи пользователем `memory_store.rs:723` принудительно
  выставляет `source_trust = 'user'`. Это легитимный путь повышения доверия
  через явное действие человека, но он означает, что после правки
  ambient-происхождение остаётся только в `provenance_source_id`.

## Содержание

- `SourceTrust::Ambient` добавляется пятым вариантом к существующему enum из
  четырёх; до этой правки его нет в коде. Правятся `as_str` и `parse`
  (`'ambient'`), а также `requires_validation()` — сейчас это
  `matches!(Self::ToolOutput | Self::Document)`, ambient добавляется в список.
  `can_ground_strict_save()` остаётся `matches!(Self::User)` и потому даёт
  `false` для ambient без правки. Колонка `source_trust` в `memory_entries` —
  `TEXT NOT NULL DEFAULT 'user'` без `CHECK` (`memory_store.rs:1088`), поэтому
  миграции для нового значения не требуется; менять `memory_store.rs` не нужно.
- Явный ранний гейт в `evaluate()`: ambient-кандидат возвращает `Pending` с
  новой причиной `AmbientNeverAutoConfirms` (правится `PolicyReason` и его
  `as_str`) сразу после проверки на секрет. Полагаться на то, что «оно и так
  упадёт ниже по коду», нельзя.
- **Порядок выключателей задан явно.** Гейт в `evaluate` стоит выше проверки
  `ExtractionDisabled`, поэтому сам по себе он пропустил бы ambient при
  выключенном общем извлечении. Решение: общий выключатель старше частного —
  при `ExtractionMode::Disabled` ambient-извлечение не запускается вовсе, и
  проверка эта делается **до** вызова `evaluate`, в ambient-точке входа.
  Гейт внутри `evaluate` остаётся второй линией обороны, а не единственной.
- Режим `EVOHIME_AMBIENT_MEMORY` (`off` | `pending`, по умолчанию `pending`).
  Значение читается и валидируется Core; неизвестное значение даёт fail-safe
  `off`, а не молчаливое включение. Аналога `open` для ambient нет.
- Триггер извлечения — закрытие эпизода, а не пользовательская фраза, со своей
  `TurnContext { user_asserted: false }`. `run_memory_extraction` сегодня
  жёстко принимает пару (user_prompt, assistant_reply) одного хода, поэтому
  ambient получает **отдельную точку входа** поверх общего `evaluate`, а не
  подделанный «ход»: подмена реплики пользователя ambient-текстом сломала бы
  смысл `user_asserted`. Диалоговый extraction остаётся на
  `detect_explicit_trigger` и не меняется.
- **Собственный гейт запуска.** Ambient-точка входа не проходит через
  `check_can_extract` в текущем виде: та отвергает всё без триггера в
  strict-режиме и заблокировала бы ambient полностью. Вместо этого ambient
  получает параллельную проверку бюджета — circuit breaker и token budget
  переиспользуются, а ветки `ModeDisabled`/`NoExplicitTrigger` заменяются на
  проверку `EVOHIME_AMBIENT_MEMORY` и общего `ExtractionMode`. Диалоговый
  `check_can_extract` при этом не ослабляется ни в одной ветке.
- Из ambient не принимаются `constraint` и `decision`: отбрасываются до
  persistence — слишком дорого ошибиться.
- Говорящий: `speaker = 'unverified'` у каждого высказывания. Диаризации и
  голосового профиля в v1 нет намеренно — голосовой шаблон это биометрия, а
  ошибка диаризации приписала бы пользователю чужое утверждение. Если в
  высказывании распознан субъект не в первом лице («он сказал», «она просила»,
  имена), `privacy_class` принудительно поднимается минимум до `sensitive`, что
  по существующему коду даёт `SensitivePrivacy`, pending и скрытое тело записи.
- Отдельные бюджеты: 6 кандидатов и 12 эпизодов в час, собственный лимит
  токенов. Переполнение — `ThrottleReason`, без очереди. Подходящих вариантов
  в enum сейчас нет, поэтому добавляются `AmbientCandidateLimit` и
  `AmbientEpisodeLimit` вместе с их `as_str`: переиспользовать `HourlyLimit`
  нельзя, иначе диалоговый и ambient-троттлинг станут неразличимы в логах.
- **`provenance_source_id = episode_id` требует правки локатора.** Сегодня это
  поле заполняется только из `RawEvidenceLocator`, где эпизода нет, поэтому
  в структуру добавляется `#[serde(default)] pub episode_id: String`, и
  `memory_provenance_source_id` проверяет его **первым**. Правка additive:
  локатор целиком в `memory_entries` не хранится, миграция не нужна. Поле
  `content_hash` для ambient остаётся пустым — по правилу 04.1 хеш текста
  приравнивается к содержимому. Без этой правки условие «удаление эпизода
  отклоняет кандидатов» осталось бы холостым (см. 04.2).
- Удаление эпизода отклоняет кандидатов причиной `source_deleted`.
- `OperationsPanel`: бейдж «услышано», подпись «говорящий не подтверждён»,
  фильтр по источнику. Механика очереди не меняется.

## Файлы

- изменить: `crates/evohime-core/src/memory_extraction.rs` (вариант
  `SourceTrust`, `as_str`/`parse`, `requires_validation`, ранний гейт в
  `evaluate`, `PolicyReason::AmbientNeverAutoConfirms`, два новых
  `ThrottleReason`, поле `episode_id` в `RawEvidenceLocator`),
  `crates/evohime-core/src/lib.rs` (ambient-точка входа с собственным гейтом
  запуска, приоритет `episode_id` в `memory_provenance_source_id`),
  `crates/evohime-core/src/memory_api.rs`,
  `desktop/evohime-electron/src/renderer/src/OperationsPanel.tsx`,
  `docs/architecture.md`.

## Проверки

- перебор всех комбинаций `kind × scope × privacy × confidence × subject` для
  `SourceTrust::Ambient` никогда не даёт `AutoConfirm`;
- ambient-извлечение действительно **запускается** в strict-режиме по умолчанию:
  тест, который упал бы, если бы ambient пошёл через `check_can_extract` и
  получил `Throttled { NoExplicitTrigger }`;
- при `ExtractionMode::Disabled` ambient-извлечение не запускается, даже если
  `EVOHIME_AMBIENT_MEMORY=pending`;
- `constraint` и `decision` из ambient не доходят до persistence;
- утверждение о третьем лице поднимает `privacy_class` и остаётся pending;
- `EVOHIME_AMBIENT_MEMORY=off` не запускает извлечение вовсе;
- кандидат, извлечённый из эпизода, получает `provenance_source_id =
  episode_id`, и удаление эпизода отклоняет его причиной `source_deleted`;
- превышение ambient-бюджета даёт `AmbientCandidateLimit`/`AmbientEpisodeLimit`,
  а не `HourlyLimit` диалогового пути;
- диалоговый extraction не изменил поведение — существующие тесты зелёные, и
  ни одна ветка `check_can_extract` не ослаблена.

## Критерии готовности

- ambient-запись не может стать активной памятью без клика пользователя;
- источник и неподтверждённость говорящего видны в UI до подтверждения;
- существующий контракт Memory Extraction не ослаблен ни в одной ветке;
- связь «кандидат ↔ эпизод» существует в данных, а не только на бумаге.
