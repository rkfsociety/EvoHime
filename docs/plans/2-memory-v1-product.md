# Подплан 2 — Memory v1: extraction и native UX

Статус: следующий после hardening
Порядок: 2 из 5
Источник: бывший единый мастер-план; актуальная детализация находится в этом подплане.

## Цель

Превратить готовые Memory domain, SQLite persistence, API и IPC-контракты в ограниченный пользовательский workflow без скрытой записи фактов.

В v1 «важной» считается запись, которая может влиять на будущие решения или поведение агента: решение, предпочтение, ограничение, правило, исправление или подтверждённый результат. Важность определяется детерминированной policy по `kind`, `privacy_label`, `confidence` и влиянию длительного сохранения, а не пользовательским флагом. Auto-save разрешён только для явно перечисленных низкорисковых kind; важные, secret-like, конфликтующие и записи с недостаточной provenance требуют подтверждения до сохранения.

## Объём

- post-run extraction фактов и решений только из bounded run evidence;
- policy для типов записей, TTL, privacy label, provenance и максимального размера;
- подтверждение пользователем важных записей до сохранения;
- native inspector UI: create, list, search, update, archive, forget, provenance;
- export/delete только через approval и audit;
- scope isolation для workspace/project/task и deterministic retrieval.

### Термины и границы v1

- Scope иерархичен: `workspace > project > task`. Запись имеет ровно один `owner_scope`; при запросе дочернего scope разрешены его собственные записи и явно разрешённые записи предков, но не записи соседних scope. Наследование не даёт права записи в родительский scope.
- Cross-scope факт не сохраняется как одна общая запись. Extractor либо создаёт отдельные кандидаты с доказательством для каждого scope, либо оставляет один кандидат в confirmation queue с обязательным выбором `owner_scope`; неоднозначный кандидат нельзя автоматически принять.
- Deterministic retrieval означает и детерминированный набор, и порядок: участвуют запрошенный scope и его предки, с precedence `task > project > workspace`; соседние scope исключаются. Из набора исключаются `archived` и `expired`, а стабильная сортировка использует `scope_depth`, точное совпадение `canonical_key`, `kind`, `updated_at` и `id`. Лимит по умолчанию — 50 результатов, заданный лимит — часть IPC-запроса и ограничен policy. В v1 нет embeddings, fuzzy- или vector-поиска.
- Stale/conflicting определяется по точному `canonical_key` и пересечению scope: устаревшая запись получает состояние `expired` и исключается из normal retrieval, а конфликтующий кандидат явно ссылается на существующую запись, хранится отдельно в confirmation queue и требует решения policy/user; автоматического merge, скрытой замены или выбора только более свежей версии нет.
- Audit — append-only журнал действий и ревизий: actor, action, record id, scope, timestamp, reason, provenance hash, результат, а для update — безопасные old/new metadata и diff значения, если это разрешено privacy policy. Для массовых операций дополнительно фиксируются operation id, полный список record id, approval и итог по каждому id. `forget` оставляет содержимое только для явного restore и исключает его из дальнейшего использования, а после `delete` содержимое и секретоподобные данные физически удаляются — остаётся только минимальный tombstone аудита.
- Offline в v1 означает транзакционную работу с локальным SQLite без обязательной сети. Удалённой синхронизации и разрешения online-конфликтов (`forget` против `archive` и т. п.) в v1 нет; это отдельный scope Memory v2.
- Lifecycle записи: `active` участвует в retrieval; `archived` сохраняется и исключается из retrieval; `expired` исключается из retrieval, но не удаляется автоматически; `forgotten` означает логическое исключение и отзыв из будущего использования с сохранением минимального audit tombstone; `deleted` означает физическое удаление содержимого после approval. `forget` и `delete` — разные операции: первая обратима только через явный restore из сохранённой версии, вторая необратима в рамках v1.

## Порядок реализации

1. Описать `MemoryCandidate` и deterministic extractor из run metrics/evidence.
2. Добавить policy decision и confirmation queue без автоматического сохранения неподтверждённых важных фактов.
3. Подключить post-run hook к Core task lifecycle после terminal outcome.
4. Добавить WinUI inspector поверх существующего IPC API.
5. Добавить integration/eval fixtures для stale, conflicting, secret-like и cross-scope записей.

## Модель кандидата, evidence и policy

Минимальный `MemoryCandidate` содержит `candidate_id`, `kind`, безопасное `content`, `scope`, `provenance`, `privacy_label`, `ttl`, `confidence`, `source_run_id`, `created_at` и `requires_confirmation`. Для детерминированного сопоставления policy также вычисляются `canonical_key`, `risk` и `policy_decision`; это производные поля контракта, а не альтернативные названия базовых полей. В provenance допускаются тип evidence и стабильный digest/идентификатор артефакта, но не полный stdout/stderr, argv, секреты или абсолютные пути вне workspace.

Allowlist bounded evidence: structured task outcome, structured tool results, explicit user/agent decisions, selected task metrics и manifest безопасных артефактов/временных файлов внутри workspace. Denylist: сырые `stdout`/`stderr`, полный `argv`, secrets, credential material, абсолютные пути вне workspace и неограниченные логи/файлы. Источник, не попавший в allowlist, не читается extractor-ом.

Правила extractor-а версионируются и задаются только конфигурацией Core из allowlisted `kind`, полей и безопасных шаблонов/регулярных выражений. Пользовательские правила могут расширять разрешённые kind и pattern-ы в пределах текущего scope, но не могут открыть denylist-источник, повысить privacy label или обойти policy. Конфигурация имеет bounded размер и проходит ту же валидацию, что и входной evidence.

Policy вычисляется детерминированно по `kind + privacy_label + confidence + persistence impact`:

| Условие | Решение | Сохранение |
| --- | --- | --- |
| низкорисковый allowlisted kind, безопасная provenance, достаточная confidence | `auto_save` | сразу в `active` |
| решение, предпочтение, ограничение или иной durable fact | `confirm` | только после `accepted` |
| secret-like, запрещённый scope, недостаточная provenance или превышение размера | `reject` | не сохраняется |
| конфликт с существующей записью | `confirm_conflict` | отдельный pending candidate с ссылкой на запись |

Решение policy сохраняется вместе с версией policy. Примеры TTL: решение и пользовательское предпочтение — 1 год; техническая диагностика с безопасной provenance — 30 дней. Точные TTL и лимиты — константы policy, покрытые тестами, а не UI. Ограничиваются и общий размер `content`, и число полей/элементов структурированного content; превышение любого лимита даёт `reject`.

Post-run extractor не обязан угадывать факты за пределами bounded evidence. Контракт evidence обязан включать структурированные метрики, безопасные tool-result summaries и manifest ссылок на допустимые артефакты/временные файлы; сырые логи остаются диагностикой и не становятся памятью. Если контекста недостаточно, создаётся кандидат с низкой confidence или запись не создаётся — скрытого расширения области чтения нет.

Confirmation queue сохраняет только безопасный кандидат и его срок действия. Её lifecycle: `pending -> accepted | rejected | expired`; переходы атомарны и попадают в audit. Pending candidates переживают restart, повторно показывают provenance до подтверждения и истекают по TTL; после `expired` они не сохраняются и не возвращаются в normal retrieval. В UI пользователь видит kind, scope, content, provenance, TTL и diff с текущей записью и может подтвердить, изменить scope/content, отложить или отклонить. Бесконечной очереди нет.

## Native UI, offline и миграции

Inspector показывает pending queue до сохранения важных записей и после подтверждения — list/search/update/archive/forget/provenance. Queue асинхронна: terminal outcome и Core task lifecycle не блокируются модальным диалогом; пользователь получает ненавязчивое уведомление и открывает очередь при удобном случае. Export и физическое удаление требуют отдельного approval, явного scope и audit confirmation; для массовых операций UI показывает число, scope и сводку объектов, а approval фиксирует полный список id и применяется атомарно.

TTL обслуживается фоновым Core cleanup job: он переводит истёкшие active/pending записи в `expired`, исключает их из retrieval и сохраняет audit, но не удаляет содержимое автоматически. Cleanup идемпотентен, ограничен batch size и повторяется после restart.

Offline все эти операции выполняются локально атомарно; при ошибке миграции SQLite восстанавливается из backup, созданного до миграции, либо миграция откатывается транзакционно. Схема имеет версию, а для поддерживаемых downgrade-сценариев поставляются проверяемые down-migration steps; отсутствие безопасного downgrade приводит к диагностической блокировке без изменения данных. Это не является механизмом отмены намеренного `forget/delete`.

Для оценки extractor-а Core пишет только агрегированные локальные метрики: число просмотренных evidence entries, число кандидатов по policy decision, долю accepted/rejected/expired, число конфликтов и длительность extraction. Содержимое фактов и секреты в телеметрию не попадают; метрики могут быть отключены privacy policy.

## Критерии готовности

- успешный и неуспешный run могут создавать bounded candidates, но важные записи требуют подтверждения;
- в памяти нет stdout/stderr, полного argv, секретов и абсолютных путей вне workspace;
- запись доступна только в своём scope и имеет provenance/TTL/privacy;
- archive/forget/export/delete корректно отображаются в UI и попадают в audit;
- schema migration rollback восстанавливает схему и данные из pre-migration backup; rollback приложения на предыдущую версию отдельно проверяет совместимость чтения и не считается заменой rollback миграции;
- одинаковый входной evidence даёт одинаковые candidates, policy decisions и порядок retrieval;
- cross-scope, stale, conflicting и secret-like кандидаты проходят описанный безопасный путь без автоматического merge или повышения scope;
- pending candidates истекают по TTL, а update/archive/forget/export/delete оставляют проверяемый audit trail;
- benchmark: extraction bounded evidence до 10 MiB укладывается в 2 секунды, retrieval до 10 000 записей — в 100 мс на локальном SQLite; storage ограничен 1 GiB на workspace с предупреждением на 80% и отказом сверх лимита.

## Зависимости

Требует завершённые Memory contracts/storage wiring и желательно метрики task runner. Полный RAG/vector search не входит: это Memory v2.

## Обязательные integration/eval cases

- один и тот же `canonical_key` в двух workspace не пересекает retrieval; cross-scope candidate требует явного выбора владельца;
- повторный extractor run даёт идентичный набор и порядок кандидатов;
- stale и conflicting записи находятся точным поиском и показываются как отдельные состояния без vector search;
- offline create/update/archive/forget/export/delete переживают перезапуск и не теряют транзакционные изменения;
- отказ миграции восстанавливает схему и данные из pre-migration backup, а намеренный `forget/delete` не восстанавливает удалённое содержимое;
- проверяется rollback приложения на предыдущую версию: старый binary читает сохранённую схему либо корректно блокируется с диагностикой, не повреждая данные;
- policy отклоняет secret-like значения, абсолютные пути вне workspace, полный argv и сырые stdout/stderr;
- негативные cases отклоняют превышение размера content, превышение числа полей, запрещённый privacy label, недопустимое пользовательское правило extractor-а и массовую операцию с чужим scope;
- cleanup переводит истёкшие записи в `expired` без физического удаления и безопасно возобновляется после restart;
- extractor metrics не содержат content, provenance secrets или абсолютные пути;
- проверяются лимиты 10 MiB/2 секунды, 10 000 записей/100 мс и квота 1 GiB на workspace.
