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

`memory_extraction.rs` реализован: `SourceTrust` из четырёх значений,
`evaluate()` с единственным путём до `AutoConfirm` (явный триггер, low risk,
`SourceTrust::User`), `pending_confirmation` по умолчанию, отказ секретам,
конфликт по `kind + canonical_subject + scope`, tombstone на `forget`.
Ambient-источника и его policy нет; `run_memory_extraction` жёстко привязана к
паре (user_prompt, assistant_reply) одного завершённого хода и другого входа
не имеет.

## Содержание

- `SourceTrust::Ambient` добавляется пятым вариантом к существующему enum из
  четырёх; до этой правки его нет в коде. Правятся `as_str` и обратный разбор
  (`'ambient'`), а также `requires_validation()` — сейчас это
  `matches!(Self::ToolOutput | Self::Document)`, ambient добавляется в список.
  `can_ground_strict_save()` остаётся `matches!(Self::User)` и потому даёт
  `false` для ambient без правки. Колонка `source_trust` в `memory_entries` —
  свободный `TEXT` без `CHECK`, поэтому миграции для нового значения не
  требуется; менять `memory_store.rs` не нужно.
- Явный ранний гейт в `evaluate()`: ambient-кандидат возвращает `Pending` с
  новой причиной `AmbientNeverAutoConfirms` сразу после проверки на секрет.
  Полагаться на то, что «оно и так упадёт ниже по коду», нельзя.
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
- Из ambient не принимаются `constraint` и `decision`: отбрасываются до
  persistence — слишком дорого ошибиться.
- Говорящий: `speaker = 'unverified'` у каждого высказывания. Диаризации и
  голосового профиля в v1 нет намеренно — голосовой шаблон это биометрия, а
  ошибка диаризации приписала бы пользователю чужое утверждение. Если в
  высказывании распознан субъект не в первом лице («он сказал», «она просила»,
  имена), `privacy_class` принудительно поднимается минимум до `sensitive`, что
  по существующему коду даёт `SensitivePrivacy`, pending и скрытое тело записи.
- Отдельные бюджеты: 6 кандидатов и 12 эпизодов в час, собственный лимит
  токенов. Переполнение — `ThrottleReason`, без очереди.
- `provenance_source_id = episode_id`; удаление эпизода отклоняет кандидатов
  причиной `source_deleted`.
- `OperationsPanel`: бейдж «услышано», подпись «говорящий не подтверждён»,
  фильтр по источнику. Механика очереди не меняется.

## Файлы

- изменить: `crates/evohime-core/src/memory_extraction.rs` (вариант enum,
  `as_str`/разбор, `requires_validation`, ранний гейт в `evaluate`),
  `crates/evohime-core/src/memory_api.rs`,
  `desktop/evohime-electron/src/renderer/src/OperationsPanel.tsx`,
  `docs/architecture.md`.

## Проверки

- перебор всех комбинаций `kind × scope × privacy × confidence × subject` для
  `SourceTrust::Ambient` никогда не даёт `AutoConfirm`;
- `constraint` и `decision` из ambient не доходят до persistence;
- утверждение о третьем лице поднимает `privacy_class` и остаётся pending;
- `EVOHIME_AMBIENT_MEMORY=off` не запускает извлечение вовсе;
- удаление эпизода отклоняет производных кандидатов;
- диалоговый extraction не изменил поведение — существующие тесты зелёные.

## Критерии готовности

- ambient-запись не может стать активной памятью без клика пользователя;
- источник и неподтверждённость говорящего видны в UI до подтверждения;
- существующий контракт Memory Extraction не ослаблен ни в одной ветке.
