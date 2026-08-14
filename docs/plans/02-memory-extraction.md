# План: Memory Extraction с подтверждением и конфликтами

Статус: revised draft для реализации после отдельного подтверждения приоритета.

## Цель и границы

Расширить Memory v1 от ручных записей и failure lessons до контролируемого
извлечения фактов, предпочтений, решений и ограничений из диалогов. Ничего не
считать долговременной памятью только потому, что это сгенерировала модель.

Первый релиз не извлекает память из всего диалога автоматически: по умолчанию
работает только `strict`-режим с явными пользовательскими триггерами («запомни»,
«важно», «ограничение» и эквивалентами). Полный extraction после каждого turn
остаётся будущим `open`-режимом и до появления Local Agentic RAG может создавать
только `pending_confirmation`, но не активную память.

## Принципы и policy gate

- Core — единственный владелец extraction, validation, policy, storage и retrieval.
- Источник имеет trust level: `user`, `tool_output`, `document`, `model_inference`.
  Только явное утверждение пользователя может быть основанием strict-mode
  сохранения; tool/document/model сами по себе никогда не дают approval.
- Любой неясный kind, subject, scope, privacy или риск переводит запись в
  `pending_confirmation`.
- `model_confidence` описывает уверенность извлекателя, а
  `verification_confidence` — только результат версионируемой verification policy.
  Повторение моделью факта confidence не повышает.
- Low-risk — явное неперсональное предпочтение формата или рабочего процесса
  (`normal` privacy); medium-risk — preference/entity, влияющие на проект;
  high-risk — constraint, decision, действие с внешним эффектом, а также
  security/health/legal/financial данные. High-risk всегда требуют approval;
  секреты не сохраняются вообще.
- В strict-режиме low-risk запись с явным триггером и достаточным confidence может
  пройти в `confirmed`; в open-режиме и для medium/high-risk обязательны
  `pending_confirmation` и approval.
- Дефолтные пороги: `model_confidence >= 0.85` для low-risk и `>= 0.95` для
  medium-risk; high-risk не автосохраняется. `verification_confidence >= 0.80`
  требуется для технического факта, если validation hook обязателен.
  Пороги версионируются в policy и конфигурируются только Core.

## Типы памяти

Добавить bounded `memory_kind`:

- `preference` — «использовать русский язык в UI», kind `preference`, scope
  `workspace`, TTL 180 дней;
- `constraint` — «не пушить без явной команды», kind `constraint`, scope
  `project`, TTL 30 дней, всегда approval;
- `decision` — «использовать Electron shell», kind `decision`, scope `project`,
  TTL 180 дней, всегда approval;
- `entity` — проверенный факт о проекте, человеке или компоненте, scope
  `project`/`workspace`, TTL 365 дней;
- `lesson` — проверенный результат выполнения, scope `project`, TTL 365 дней;
- `session_summary` — краткая рабочая сводка, scope `session`, живёт до конца
  сессии и ещё 24 часа, не участвует в long-term retrieval и не может быть
  promoted в persistent memory без отдельного явного подтверждения.

TTL, длины и квоты являются конфигурируемыми параметрами среды, но указанные
значения — обязательные production defaults. Истёкшая запись получает state
`expired`, исключается из retrieval и может быть продлена только явным действием
пользователя или новой проверкой.

## Этапы

### 02.1 Схема и доменный контракт

- Добавить `kind`, `scope`, `canonical_subject`, `statement`,
  `model_confidence`, `verification_confidence`, `confirmation_state`,
  `privacy`, `source_trust`, `supersedes`, `superseded_by`,
  `supersession_reason`, `extractor_version`, `policy_version`,
  `validation_status`, `validated_at`, `expires_at` и provenance.
- States: `candidate` (результат model call до policy gate),
  `pending_confirmation`, `confirmed`, `rejected`, `superseded`, `expired`,
  `forgotten`. Pending-конфликт не меняет старую активную запись; supersede
  происходит только после явного выбора пользователя.
- Migration Memory v1 сохраняет исходный statement и provenance. Явные
  failure lessons получают `kind=lesson`, прочие старые факты — `kind=entity`;
  все legacy rows получают `state=confirmed`, `extractor_version=v1_legacy`,
  `policy_version=legacy-v1`, `model_confidence=1.0`,
  `verification_confidence=1.0`, `supersedes=NULL`, `superseded_by=NULL`.
  Миграция транзакционная, с backup и rollback-тестом.
- Добавить индексы по `kind`, `scope`, `confirmation_state`,
  `(canonical_subject, scope)`, `expires_at` и provenance source id.
- Ограничения по умолчанию: statement 4096 UTF-8 символов, provenance 8192
  байт, structured output 16 KiB, не более 5 candidates на turn и 30
  candidates в час на workspace. Oversized output отклоняется без тримминга
  смысла и без сжатия, причина попадает в diagnostics.
- Privacy enum: `normal`, `sensitive`, `secret`. `secret` отвергается до
  persistence; `sensitive` не попадает в обычный audit/body response и
  маскируется в renderer.

### 02.2 Canonical subject, scope и provenance

- Canonical subject строится детерминированным нормализатором с версией:
  Unicode NFKC, case-folding, пробелы/пунктуация, локальные aliases и
  зарегистрированные entity ids. Синонимы и multilingual aliases хранятся в
  таблице aliases; model inference не может единолично создать alias.
- Если нормализация неоднозначна или entity linking не подтверждён, конфликт
  не разрешается автоматически, а candidate становится pending.
- Приоритет scope для retrieval: `task` > `project` > `workspace` > `session`.
  Более узкая запись не уничтожает широкую: она действует только в своём scope.
  Равные scopes с несовместимыми statements образуют conflict.
- Устойчивый provenance locator использует `message_id`/`task_id`,
  `tool_call_id`, а для файлов — логический path, content hash и line range.
  В provenance не копируются секреты или полный body без policy.

### 02.3 Извлекатель

- Core запускает bounded model call после explicit trigger в strict mode или
  после завершения turn в open mode. В open mode результат всегда проходит
  `pending_confirmation`.
- Контекст ограничен последними 10 сообщениями и 2048 токенами; при превышении
  берётся минимальный релевантный фрагмент, а не весь диалог. Provider secrets
  не отправляются.
- Structured JSON обязан содержать `kind`, `statement`, `scope`,
  `canonical_subject`, обе confidence, `reason`, `evidence_locator`,
  `privacy`, `source_trust` и `suggested_ttl`; неизвестные поля отклоняются.
- На malformed/невалидный output — максимум 2 повтора с задержками 250 и 1000
  мс; причина каждого отказа логируется без содержимого. Три malformed output
  за 10 минут включают circuit breaker на 15 минут. Отдельный extraction budget:
  не более 100 000 input/output токенов в час на workspace.
- Extraction асинхронен и не блокирует основной ответ; target — не более +200 мс
  p95 к основному turn. При недоступной модели задача продолжается без памяти.

### 02.4 Подтверждение, validation и асинхронность

- `constraint` и `decision`, влияющие на действия, всегда требуют approval;
  если признак влияния не доказан policy — считать его влияющим.
- UI показывает statement, source trust, evidence, scope, TTL, privacy,
  confidence и предполагаемый conflict. Доступны `сохранить`, `отклонить`,
  `изменить`, `только на эту сессию`, а также batch confirm/reject.
- «Только на эту сессию» хранится как отдельный session-scoped state с
  автоматическим expiry, не создаёт persistent row и не попадает в long-term
  retrieval.
- При offline/background сценарии pending остаётся pending до следующего
  взаимодействия или batch review; автоматического reject/approve нет.
  Неразрешённый conflict оставляет старую активную запись и новую pending.
- Verification hook возвращает `{valid, confidence, checked_at,
  validator_version, evidence_digest, reason}`. Таймауты: filesystem/git 2 с,
  tool/API 5 с; одна повторная проверка. `unknown` оставляет pending,
  `invalid` исключает запись из retrieval и требует новой проверки/решения.
  Изменение file hash, git revision или tool version инвалидирует прошлую
  проверку.
- Отсутствующий валидатор — это `unknown`, а не отказ и не подтверждение.
  Файловую evidence проверяет сверка content hash, доступная сразу; tool/API
  и document evidence проверяет Local Agentic RAG (план 03), поэтому до него
  такие записи остаются pending. Ни один класс проверки не имеет права
  подтвердить запись, которую он фактически не проверил.
- Verification policy версионируется и является единственным механизмом,
  который повышает verification confidence. Approval пользователя фиксируется
  отдельным audit event.

### 02.5 Конфликты и забывание

- Conflict определяется по `kind + canonical_subject + scope` и несовместимым
  statements. Chain `A -> B -> C` хранится через supersedes/superseded_by и
  `supersession_reason`; выбор пользователя обязателен.
- Retrieval отдаёт только актуальную запись, подходящую по scope и validation,
  плюс компактную provenance/conflict chain. Истёкшие, forgotten и invalid
  записи не участвуют.
- Forget — это logical deletion с очисткой statement/body, evidence,
  embeddings, vector-index entries, search/cache entries и derived contexts.
  В audit/tombstone остаются только случайный id, kind, scope, timestamps,
  reason class и digest без исходного текста; логи и новые backups не содержат
  body. Старые encrypted backups очищаются при ближайшей ротации, default
  retention для них — 7 дней.
- Forget активной записи не восстанавливает superseded автоматически. UI может
  показать chain и предложить явной командой восстановить старую запись после
  новой проверки.

## IPC и UI

- Additive IPC: `ListMemory`, `GetMemory`, `GetConflicts`,
  `ConfirmMemory`, `RejectMemory`, `SupersedeMemory` и batch-варианты.
- `ListMemory` по умолчанию возвращает только metadata, cursor pagination и
  bounded page size; body доступен только через явный `GetMemory` с bounded
  limit и privacy redaction.
- Core проверяет caller role, workspace/scope authorization и idempotency key.
  Повторные confirm/reject безопасны и возвращают фактическое текущее state;
  concurrent actions сериализуются транзакцией.
- OperationsPanel показывает pending count, conflict count, TTL/expired count,
  source trust, предложения разрешения и цепочку supersede без раскрытия
  sensitive body.

## Проверки

- migration tests: Memory v1 mapping, индексы, backup и rollback;
- extraction contract tests: valid/malformed/unknown/oversized JSON, retry,
  circuit breaker, rate limit и strict/open modes;
- policy tests: risk classes, thresholds, trust levels, privacy redaction и
  fallback в pending;
- canonicalization tests: aliases, multilingual subjects, scope precedence и
  unresolved entity;
- approval tests: single/batch, session-only, offline pending и idempotency;
- validation tests: timeout, retry, invalidation по hash/revision и negative result;
- conflict tests: A -> B -> C, concurrent confirmations и conflict during pending;
- forget tests: SQLite, export, search, logs, backups, caches, embeddings и
  derived context не содержат body; tombstone содержит только разрешённые metadata;
- restart/crash-recovery tests между model call, persistence и confirmation;
- concurrency/stress tests для большого pending queue, чтения/записи и batch UI;
- performance test подтверждает extraction p95 overhead <= 200 мс;
- operational metrics: false-save rate, candidate rejection rate, confirmation
  latency, unresolved conflicts, extraction failure rate, validation invalid rate
  и pending queue size.

## Критерии готовности

- Ни один model-generated candidate не становится активной памятью без strict
  trigger + policy или явного approval.
- Пользователь может полностью отключить automatic extraction; ручные команды
  «запомни» продолжают работать.
- `session_summary` не promoted в persistent memory без отдельного approval.
- Каждая активная запись объясняет source trust, provenance, scope, privacy,
  confidence, policy/version и TTL.
- Expiry, forget, conflict resolution и validation не оставляют скрытого
  retrieval или действия по старому содержимому.
- IPC не раскрывает body по умолчанию, повторные действия идемпотентны, а
  concurrent state transitions транзакционно согласованы.
- Extraction failure или недоступный validator не ломает основную задачу.
- Все migration, policy, security, conflict, recovery, stress и performance
  tests проходят на текущем Windows checkout.

## Зависимости

Блокирующие: существующие Memory v1, approval, SQLite migrations и
authenticated named-pipe IPC. Все они уже есть, поэтому план выполним
целиком и не ждёт других планов.

Опциональные интеграции, не блокирующие этот план:

- Local Agentic RAG (план 03) даёт validation для document facts. **До его
  появления кандидаты с `source_trust = document` или `tool_output` получают
  `validation_status = unknown` и остаются в `pending_confirmation`.** Это
  штатное поведение, а не незавершённая работа: непроверяемый факт не
  подтверждается вслепую и не попадает в retrieval. Никакой другой части
  плана RAG не требуется.
- Context Budget Manager (план 01) даёт context ledger. До его появления
  происхождение записи объясняют provenance locator и audit event; ledger
  добавляет ту же связь на уровне выбора контекста.

Extraction model — отдельная bounded лёгкая модель, не основная модель агента.
HTTP server, browser launcher и внешний Node.js runtime не нужны.
