# Подплан 2 — Memory v1: extraction и native UX

Статус: следующий после hardening
Порядок: 2 из 5
Источник: бывший единый мастер-план; актуальная детализация находится в этом подплане.

## Цель

Превратить готовые Memory domain, SQLite persistence, API и IPC-контракты в ограниченный пользовательский workflow без скрытой записи фактов.

В v1 «важной» считается запись, которая может влиять на будущие решения или поведение агента: решение, предпочтение, ограничение, правило, исправление или подтверждённый результат. Важность определяется policy по типу записи, privacy label, provenance и оценке риска, а не только пользовательским флагом. Низкорисковые записи могут сохраняться автоматически только если это явно разрешено policy; важные, secret-like, конфликтующие и записи с недостаточной provenance всегда попадают на подтверждение до сохранения.

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
- Deterministic retrieval означает и детерминированный набор, и порядок: сначала точное совпадение нормализованных ключей/полей в разрешённых scope, затем стабильная сортировка по `scope_depth`, `type`, `updated_at` и `id`. В v1 нет embeddings, fuzzy- или vector-поиска.
- Stale/conflicting определяется по точному `canonical_key` и пересечению scope: устаревшая запись помечается при истечении TTL, а конфликтующая остаётся отдельной версией с provenance и требует решения пользователя; автоматического merge нет.
- Audit — append-only журнал действий и ревизий: actor, action, record id, scope, timestamp, reason, provenance hash и результат. Для update сохраняются предыдущие версии метаданных/значений, если это разрешено privacy policy; после `forget/delete` содержимое и секретоподобные данные физически удаляются, остаётся только минимальный tombstone аудита.
- Offline в v1 означает транзакционную работу с локальным SQLite без обязательной сети. Удалённой синхронизации и разрешения online-конфликтов (`forget` против `archive` и т. п.) в v1 нет; это отдельный scope Memory v2.

## Порядок реализации

1. Описать `MemoryCandidate` и deterministic extractor из run metrics/evidence.
2. Добавить policy decision и confirmation queue без автоматического сохранения неподтверждённых важных фактов.
3. Подключить post-run hook к Core task lifecycle после terminal outcome.
4. Добавить WinUI inspector поверх существующего IPC API.
5. Добавить integration/eval fixtures для stale, conflicting, secret-like и cross-scope записей.

## Модель кандидата и policy

Минимальный `MemoryCandidate` содержит `candidate_id`, `type`, `canonical_key`, безопасное `value`, `owner_scope`, `source_run_id`, bounded `provenance`, `privacy_label`, `confidence`, `ttl`, `risk` и `policy_decision`. В provenance допускаются тип evidence и стабильный digest/идентификатор артефакта, но не полный stdout/stderr, argv, секреты или абсолютные пути вне workspace.

Примеры policy: решение и пользовательское предпочтение требуют подтверждения и получают TTL 1 год; техническая диагностика с безопасной provenance может сохраняться автоматически на 30 дней; secret-like и cross-scope без выбранного владельца запрещаются; превышение максимального размера отклоняется. Точные TTL и лимиты должны быть константами policy, покрыты тестами и изменяться версией policy, а не зашиваться в UI.

Post-run extractor не обязан угадывать факты за пределами bounded evidence. Контракт evidence обязан включать структурированные метрики, безопасные tool-result summaries и manifest ссылок на допустимые артефакты/временные файлы; сырые логи остаются диагностикой и не становятся памятью. Если контекста недостаточно, создаётся кандидат с низкой confidence или запись не создаётся — скрытого расширения области чтения нет.

Confirmation queue сохраняет только безопасный кандидат и его срок действия. В UI пользователь видит тип, scope, значение, provenance, TTL и diff с текущей записью и может подтвердить, изменить scope/value, отложить или отклонить. Не отвеченные кандидаты автоматически истекают по TTL и попадают в audit; бесконечной очереди нет.

## Native UI, offline и миграции

Inspector показывает pending queue до сохранения важных записей и после подтверждения — list/search/update/archive/forget/provenance. Export и физическое удаление требуют отдельного approval, явного scope и audit confirmation. Offline все эти операции выполняются локально атомарно; при ошибке миграции SQLite восстанавливается из backup, созданного до миграции, либо миграция откатывается транзакционно. Это не является механизмом отмены намеренного `forget/delete`.

## Критерии готовности

- успешный и неуспешный run создают bounded candidates, но важные записи требуют подтверждения;
- в памяти нет stdout/stderr, полного argv, секретов и абсолютных путей вне workspace;
- запись доступна только в своём scope и имеет provenance/TTL/privacy;
- archive/forget/export/delete корректно отображаются в UI и попадают в audit;
- migration rollback и offline operation проходят без потери существующих записей;
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
- policy отклоняет secret-like значения, абсолютные пути вне workspace, полный argv и сырые stdout/stderr;
- проверяются лимиты 10 MiB/2 секунды, 10 000 записей/100 мс и квота 1 GiB на workspace.
