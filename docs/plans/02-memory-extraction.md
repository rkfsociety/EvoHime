# План: Memory Extraction с подтверждением и конфликтами

Статус: draft для ревью.

## Цель

Расширить Memory v1 от ручных записей и failure lessons до контролируемого
извлечения фактов, предпочтений, решений и ограничений из диалогов. Ничего
не считать долговременной памятью только потому, что это сгенерировала модель.

## Принципы

- Core — единственный владелец extraction, validation, storage и retrieval.
- Автоматически сохраняются только низкорисковые и явно подтверждённые записи.
- Сомнительные сведения сначала попадают в pending queue.
- Каждая запись имеет provenance, confidence, privacy, scope и lifecycle.
- Forget удаляет содержимое, а conflict не уничтожает историю без явной политики.

## Типы памяти

Добавить bounded `memory_kind`:

- `preference` — устойчивое предпочтение пользователя;
- `constraint` — ограничение, которое нужно соблюдать;
- `decision` — принятое решение проекта;
- `entity` — устойчивый факт о проекте, человеке или компоненте;
- `lesson` — проверенный опыт выполнения;
- `session_summary` — краткая память текущей сессии.

## Этапы

### 1. Схема и доменный контракт

- Добавить kind, confidence, confirmation state, supersedes/superseded_by,
  extractor version и validation status в SQLite migration.
- Сохранить обратную совместимость старых Memory v1 rows с defaults.
- Ввести states `candidate`, `pending_confirmation`, `confirmed`, `rejected`,
  `superseded`, `forgotten`.
- Ограничить длину, TTL, число candidates на scope и размер provenance.

### 2. Извлекатель

- Добавить Core-only extraction stage после завершения user turn или task,
  используя отдельный bounded model call.
- На вход давать только минимальный диалоговый фрагмент и существующие
  релевантные memory ids; не отправлять provider secrets.
- Требовать structured JSON: kind, statement, scope, confidence, reason,
  evidence locator, privacy и suggested TTL.
- При malformed output отклонять candidate, не повторять бесконечно.

### 3. Подтверждение

- `constraint` и `decision`, влияющие на действия, всегда требуют approval.
- `preference` с confidence ниже порога требует подтверждения.
- UI показывает: что будет сохранено, источник, область действия, TTL и
  предполагаемый конфликт.
- Добавить варианты `сохранить`, `отклонить`, `изменить`, `только на эту сессию`.
- Решение пользователя записывать как отдельный audit event.

### 4. Confidence и validation

- Разделить model confidence и verification confidence.
- Подтверждённый tool result может повысить verification confidence; одно лишь
  повторение моделью confidence не повышает.
- Для технических фактов поддержать validation hook: filesystem/git/tool/API
  должен подтвердить факт до promotion.
- Неподтверждённые или противоречивые candidates не использовать в системном
  контексте, пока не пройдут policy gate.

### 5. Конфликты и забывание

- Определять конфликт по kind + canonical subject + scope.
- Новая подтверждённая запись supersedes старую; старую не удалять физически.
- При неоднозначности показывать пользователю обе версии и не выбирать молча.
- Retrieval должен отдавать только актуальную запись и компактную provenance
  цепочку.
- Forget должен очищать candidate, confirmed record, provenance body и
  embeddings/index entries, если они появятся.

## IPC и UI

- Additive IPC: `ListMemory` фильтры kind/status/confidence и `ConfirmMemory`,
  `RejectMemory`, `SupersedeMemory`.
- Не передавать memory body в renderer без явного read request и bounded limit.
- В OperationsPanel показывать pending confirmations, конфликты, TTL и источник.

## Проверки

- migration tests с существующими Memory v1 rows;
- extraction contract tests на valid/malformed/oversized JSON;
- tests на redaction до model call и до persistence;
- approval tests для decision/constraint;
- conflict tests с одинаковым subject в project/task/workspace scope;
- forget tests, проверяющие отсутствие содержимого в SQLite, export и search;
- replay tests: restart между candidate и confirmation не теряет state.

## Критерии готовности

- ни один model-generated candidate не становится активной памятью без policy;
- каждая активная запись объясняет источник и confidence;
- конфликтующие инструкции не приводят к непредсказуемому поведению;
- пользователь может увидеть, исправить и забыть память;
- extraction failure не ломает основную задачу.

## Зависимости

Использует существующий Memory v1, approval, SQLite migrations и context ledger.
Для document facts нужен Local Agentic RAG validation. До этого extraction
можно включить только для явных пользовательских фраз вроде «запомни».
