# Этап 04.7: Ограниченная проактивность

Этап плана [04 Постоянное слушание и ambient-память](04-0-ambient-listening.md).

## Зависимости

Блокирующие: этап 04.4 — источник сигнала; этап 04.5 — карточки и
`ResolveAmbientProposal`; этап 04.6 — кандидаты и их provenance.

Подписанные receipts плана 01 уже являются частью текущего runtime-контракта.
Принятие предложения проходит существующий receipt/approval путь; новый
ambient-специфичный receipt в этом этапе не создаётся.

Это последний этап плана 04.

## Что этап отдаёт наружу

Механизм, которым Ева сама предлагает действие по услышанному, с жёстким
потолком и обязательным кликом пользователя.

## Что уже есть в коде

`crates/evohime-supervisor/src/schedule_contract.rs` даёт готовую семантику
`TriggerKind::Event`, лизы, retry и dead-letter, но `runtime_loop.rs:1–10`
прямо документирует, что состояние живёт только в памяти процесса супервизора
(«State lives only for the supervisor process's lifetime; there is no
persistence backing it yet»); сам супервизор не имеет доступа ни к модели, ни к
SQLite. Поэтому берётся семантика и bounded-стиль этих контрактов, а исполнение
живёт в Core.

Сверено дополнительно, потому что этап повторяет эти образцы:

- `OperationsPanel` — event-driven проекция: список строится из полезной
  нагрузки события `memory.pending` (`OperationsPanel.tsx:126`), а действия
  идут отдельными командами `core.confirmMemory` / `core.rejectMemory`
  (`:190`);
- в коде уже действует ровно то правило приватности, которое ambient обязан
  повторить: у `MemoryMetadata` **намеренно нет поля `statement`**, потому что
  «`memory.pending` and `memory.conflicts` never carry a body, so the panel
  cannot leak one even by accident» (`OperationsPanel.tsx:14–18`);
- решения по памяти уже идемпотентны: `core.confirmMemory`/`core.rejectMemory`
  несут `approvalId` и `idempotencyKey` (`api.ts:375`, вычисляются в
  `OperationsPanel.tsx:191`).

## Содержание

- `crates/evohime-core/src/ambient_proactivity.rs` — side-effect-free автомат:
  `Proposal { proposal_id, proposal_key, mute_key, kind, source_episode,
  created_at, state }`, состояния `Proposed | Accepted | Declined | Muted |
  Expired`, окно жизни 24 часа.
- **Два ключа, а не один.** Дедупликация идёт по `proposal_key` = `kind` +
  `canonical_subject` + округлённое время: он и стоит под `UNIQUE`. Постоянный
  mute («больше не предлагать такое») идёт по `mute_key` = `kind` +
  `canonical_subject`, **без времени**. Один ключ на обе роли не работает: с
  временем в ключе mute заглушил бы ровно одну временную корзину и молча
  перестал бы действовать через час, а без времени `UNIQUE` запретил бы любое
  повторное предложение по той же теме после истечения предыдущего.
- Персистентность предложений — additive-таблица в отдельной миграции **v26**,
  следующей за ambient-хранилищем v25 из 04.2. Она сохраняет предложения,
  mute-ключи и счётчики бюджета; миграция транзакционна и получает обычный
  backup до изменения схемы. Минимальные ограничения схемы: уникальный
  `proposal_id`, уникальный `proposal_key`, отдельная таблица mute по
  `mute_key`, nullable `source_episode_id` с `ON DELETE SET NULL`, пара
  nullable-полей `source_deleted_at`/`source_deleted_reason` под `CHECK`
  «оба NULL либо оба заполнены» (пока источник жив, они пусты — «обязательными»
  их называть нельзя), `state` только из пяти состояний автомата, а счётчики
  бюджета — одна строка на ambient-профиль.
- **Порядок операций при удалении источника задан явно.** Удаление эпизода
  переводит связанные предложения в `Expired` с причиной `source_deleted` — и
  этот `UPDATE` выполняется **до** удаления строки эпизода, в той же
  транзакции с его tombstone. Наоборот нельзя: `ON DELETE SET NULL` сработает
  первым и обнулит связь, после чего найти затронутые предложения будет уже
  нечем. FK при этом не блокирует удаление источника.
- Закрытый список разрешённых эффектов:
  1. карточка-предложение в UI;
  2. неисполняемое `pending`-напоминание.

  Всё остальное — запуск задачи, вызов инструмента, запись файла, сеть —
  запрещено инвариантом и покрыто негативными тестами. Это не настройка.
- Потолки (`ProactivityBudget` из 04.1): не более 3 предложений в час и 10 в
  сутки, не менее 10 минут между двумя предложениями, тихие часы и чёрный
  список из общей политики. `ProactivityBudget` неизменяем, а текущие
  счётчики (`hour_count`, `day_count`, `last_proposed_at`) живут в
  `AmbientProactivityRegistry` (04.1) и персистятся строкой таблицы v26 —
  общего `CoreState` в коде нет. Превышение потолка — предложение
  **отбрасывается** со счётчиком, а не копится в очередь: иначе после часа
  тишины пользователь получит десять карточек разом.
- Дублирующее предложение поднимает счётчик у существующей карточки, а не
  создаёт вторую.
- **Событие `ambient.proposal`.** 04.5 перечисляет четыре ambient-события
  (`state`, `engine`, `transcript`, `retention`); это пятое, и оно добавляется
  здесь — renderer подписывается на него дополнительно. Payload несёт только
  `proposal_id`, `kind`, bounded `canonical_subject` и `state`. Текста
  предложения в нём нет: карточка производна от речи, а `events` —
  append-only durable-таблица, из которой всё ambient-содержимое пришлось бы
  вычищать (правило 04.2). Прецедент прямой: `memory.pending` по той же
  причине не несёт `statement`. Человекочитаемый текст карточки renderer
  получает командой, а `ambient.proposal`-строки удаляются вместе с эпизодом
  тем же механизмом, что остальные `ambient.*`-строки журнала.
- Принятие предложения создаёт обычную задачу или неисполняемое напоминание
  через штатный механизм Core с сохранением `provenance_source_id` и проходит
  штатный approval-путь. Поскольку принятие создаёт задачу, `ResolveAmbientProposal`
  из 04.5 получает additive-поле `idempotency_key` — по образцу
  `core.confirmMemory`: без него двойной клик по карточке породит две задачи.
  Пока предложение не принято, оно видно в `OperationsPanel` как `pending` с
  признаком ambient-источника.

## Файлы

- создать: `crates/evohime-core/src/ambient_proactivity.rs`;
- изменить: `crates/evohime-core/src/lib.rs`,
  `crates/evohime-core/src/ipc_bridge.rs`,
  `crates/desktop-ipc/proto/evohime.desktop.proto` (поле `idempotency_key` в
  `ResolveAmbientProposal`) с регенерацией
  `desktop/evohime-electron/src/main/ipc/generated/protocol.{js,d.ts}`,
  `crates/evohime-local-storage/src/lib.rs` (миграция v26),
  `crates/evohime-local-storage/src/ambient_store.rs`,
  `desktop/evohime-electron/src/shared/api.ts`,
  `desktop/evohime-electron/src/renderer/src/ListeningPanel.tsx`,
  `desktop/evohime-electron/src/renderer/src/OperationsPanel.tsx` (карточка
  предложения в очереди),
  `docs/architecture.md`.

Ambient-предложения повторяют схему `OperationsPanel`: собственное событие
`ambient.proposal` со списком предложений плюс команда `ResolveAmbientProposal`
из 04.5, а не подмешивание в `memory.pending`.

## Проверки

- потолки частоты и суточного числа соблюдаются на детерминированных часах;
- дублирующее предложение поднимает счётчик, а не создаёт вторую карточку;
- mute по `mute_key` продолжает действовать и после смены временной корзины,
  то есть заглушает предложение, чей `proposal_key` уже другой;
- после истечения предложения новое предложение по той же теме создаётся и не
  упирается в `UNIQUE(proposal_key)`;
- негативные тесты: попытка проактивно вызвать инструмент, записать файл или
  выйти в сеть отклоняется до эффекта;
- отсутствие реакции 24 часа переводит предложение в `Expired`;
- удаление эпизода-источника переводит предложения в `Expired` с причиной
  `source_deleted`, а не оставляет их с обнулённой ссылкой;
- `ambient.proposal` ни при каких входных данных не содержит текста
  предложения или высказывания;
- повторный `ResolveAmbientProposal` с тем же `idempotency_key` не создаёт
  вторую задачу;
- mute переживает рестарт Core;
- принятие проходит обычный approval и видно в timeline.

## Критерии готовности

- Ева может напомнить по услышанному, но не может ничего сделать сама;
- потолок частоты доказуем тестом и не обходится накоплением очереди;
- каждое предложение прослеживается до эпизода-источника и исчезает вместе с
  ним;
- ни одна durable-строка журнала не хранит содержимое предложения.
