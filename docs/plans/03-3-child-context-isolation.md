# Этап 03.3: Context isolation

Этап плана [03 Специализированные child workflows](03-0-specialized-child-workflows.md).

## Зависимости

Блокирующие: этап 03.2 — состояния, в которых живёт контекст ребёнка, и
`coordinator_child_checkpoint`, где 03.3 хранит offload-related поля отчёта
(`evidence_locators_json`). Этап 03.1 — `Grant`/`is_subset_of` и
`ChildBudget`/`Schema.max_bytes`, на которые 03.3 опирается для offload
threshold и escalation grant, но не переопределяет их формат.

Базовые scratchpad (`crates/context-budget/src/scratchpad.rs`) и
content-addressed Artifact Store (`crates/context-budget/src/artifact.rs`,
`crates/evohime-local-storage/src/artifact_store.rs`, schema v19) уже
реализованы и не меняются этим этапом: `ArtifactRef`, `access_allowed`,
`Privacy`, `ArtifactError`, `bounded_summary`, `plan_eviction` — существующий
код. 03.3 — это wiring: какие grants получает какая роль, когда offload
триггерится в child workflow и как reviewer получает summary-only доступ. Это
явно разделяет 03.1–03.3: 03.1 — typed grant/budget contract сам по себе,
03.2 — lifecycle и checkpoint, куда попадают locator'ы, 03.3 — кто из ролей
какой locator может прочитать и с каким уровнем детализации.

Разблокирует: 03.4.

## Что этап отдаёт наружу

Изоляцию контекста между детьми и offload больших результатов, реализованные
поверх существующего Artifact Store и `Grant` из 03.1.

## Определения

- **Selected context** — явный список `context item id`/`artifact locator`,
  который coordinator указывает в `input context ids` контракта child task
  (см. [03-0, «Контракт child task»](03-0-specialized-child-workflows.md#контракт-child-task)).
  Child не получает полный context родителя — только перечисленные id;
  формирование списка остаётся ответственностью coordinator, 03.3 добавляет
  только проверку на границе создания child: любой id/locator в `input
  context ids`, не разрешённый `access_allowed` для этого child (см. ниже),
  отклоняется до создания child с `ContractError`-подобной ошибкой (новый
  вариант `ContextIdNotAccessible`, симметричный `GrantEscalation` из 03.1).

- **Task namespace** — цепочка владения задачей: `owner_task_id` артефакта
  плюс `parent_chain` (список id родительских задач вплоть до корня). Это уже
  реализованный концепт — `access_allowed(reference, task_id, parent_chain)`
  в `crates/context-budget/src/artifact.rs:120-125` разрешает доступ, если
  `task_id` — владелец или один из `parent_chain` — владелец. 03.3 не вводит
  новую структуру, а формализует термин и требует, чтобы `parent_chain`,
  который Core передаёт в `access_allowed` при чтении, строился из
  `CorrelationContext` (03.1), а не собирался ad hoc на каждом call site.

- **Offload threshold («большой результат»)** — report offload'ится в
  Artifact Store вместо inline-передачи, если сериализованный `output_data`
  превышает `Schema.max_bytes` запроса (03.1) или, если `output_schema` не
  задан, дефолт `ChildBudget`-независимый `DEFAULT_INLINE_MAX_BYTES = 32 *
  1024` байт (константа этого этапа, не 03.1 — offload trigger относится к
  context isolation, а не к contract validation). Offload — не отдельная
  metric-based эвристика, а тот же лимит, что уже проверяется в 03.1 до
  persistence; 03.3 добавляет только ветвление: превышение лимита не
  отклоняет report (как в 03.1), а вызывает `TaskArtifactStore::write` и
  заменяет `output_data` на `ArtifactRef{locator, content_hash, summary}` в
  отчёте, который видит parent.

- **Policy grant для чужого namespace/full artifact** — это тот же `Grant`
  из `child_contracts.rs` (03.1), не новая структура. 03.3 вводит два
  конкретных `grant_type`:
  - `"artifact_read_summary_only"` — выдаётся reviewer по умолчанию при
    создании (см. «Reviewer» ниже);
  - `"artifact_read_full"` со `scope = locator` — выдаётся coordinator
    явно, когда полный артефакт нужен (например, reviewer запрашивает
    diagnostic). Проверяется тем же `is_subset_of`/`validate_grant_subset`,
    что и остальные grants в 03.1, на каждом tool call, читающем артефакт.

## Содержание

- **Selected context на создании child.** Coordinator передаёт `input
  context ids`; Core проверяет каждый id/locator через `access_allowed`
  относительно `task_namespace` создаваемого child до persistence запроса.
  Child не получает доступа к context item'ам родителя, не перечисленным в
  этом списке, даже если они технически доступны по `task_namespace`
  (allowlist, не полный namespace access).

- **Offload больших результатов.** Report, чей `output_data` превышает
  offload threshold, offload'ится через существующий `TaskArtifactStore`
  (`crates/evohime-local-storage/src/artifact_store.rs`) с `privacy`,
  унаследованным от declared privacy данных в отчёте (не всегда
  `Workspace`); parent получает `ArtifactRef.summary`, построенный
  `bounded_summary` (уже реализовано, не переписывается), plus `locator` и
  `content_hash`. Это исключение из inline-валидации 03.1 и должно быть явно
  включено в child contract; без такого флага действует обычный
  `OutputTooLarge`. Для этого этап добавляет optional additive-поля typed
  report `output_privacy` и `output_artifact`; их отсутствие означает обычный
  inline output и сохраняет совместимость с контрактом 03.1. Данные с
  `Privacy::Sensitive`/`Privacy::Secret` не offload'ятся —
  `ArtifactError::PrivacyForbidsOffload` (уже реализовано в `artifact.rs`)
  отклоняет передачу результата родителю. Такой результат не остаётся inline
  в parent-visible report и не появляется в summary, diagnostics или trace;
  child может завершиться только с redacted reason без raw payload. 03.3
  добавляет проверку этого пути
  специально для child report (сейчас `PrivacyForbidsOffload` проверяется
  только в generic artifact write, не в report offload branch).

- **Locator access на каждом чтении.** Переиспользуется существующий
  `access_allowed` — 03.3 не меняет его логику, а гарантирует, что Core
  вызывает его на каждом read tool call (а не только при первом обращении),
  используя актуальный `parent_chain` из `CorrelationContext`, а не
  кэшированный на момент создания child (тот же принцип grant drift, что и
  в 03.1 для tool-call grants).

- **Reviewer — summary-only по умолчанию.** При создании reviewer child
  coordinator обязан выдать `Grant{grant_type:
  "artifact_read_summary_only"}` без явного `"artifact_read_full"`.
  Core-уровень enforcement: tool, читающий полный артефакт
  (`TaskArtifactStore::read`), проверяет наличие
  `"artifact_read_full"`-grant со scope, покрывающим запрошенный locator;
  без него доступен только `get_ref` (locator + summary + hash, без
  содержимого) — тот же вызов, что уже возвращает `ArtifactRef` без чтения
  blob. Расширение до full-grant — отдельное явное действие coordinator, не
  побочный эффект другого approval.

- **Reviewer не может писать.** Не новый механизм: role permission matrix
  (03-0) уже не включает write/commit capability для `reviewer`; 03.3
  формулирует явно, что доступ к diff/evidence (через selected context или
  offloaded artifact) не расширяет `requested_capabilities` — Core policy
  проверяет capability отдельно от context/artifact access на каждом tool
  call (тот же tool-call boundary, что для grants в 03.1).

- **Секреты не переходят соседнему child/role без grant.** Гарантируется
  двумя уже существующими механизмами вместе: (1) `Privacy::Secret`/
  `Sensitive` запрещает offload (см. выше), поэтому секрет физически не
  попадает в Artifact Store, откуда мог бы утечь через locator access; (2)
  `access_allowed` ограничивает даже non-secret локаторы task namespace'ом.
  03.3 закрывает единственный оставшийся путь утечки: `summary`, который
  Core кладёт в `ArtifactRef.summary` через `bounded_summary`, строится из
  того же `output_data`, что помечено `Privacy` — summary не генерируется
  для `Secret`/`Sensitive` содержимого вообще (offload для них blocked
  раньше, см. выше), так что вопрос "не содержит ли summary секрет" снимается
  на уровне privacy label, а не отдельной санитизацией текста summary.

## Обработка ошибок

- **Selected context id недоступен создаваемому child** — создание
  отклоняется до persistence с `ContractError::ContextIdNotAccessible`
  (аналогично `GrantEscalation` в 03.1); не является revision — child не
  создан.
- **Offload отклонён (`PrivacyForbidsOffload`/`QuotaExceeded`)** — report не
  offload'ится, `TypedChildReport` отклоняется тем же путём, что stale
  provenance в 03.1 (`ContractError`-класс ошибка до persistence); child
  может переотправить report в пределах `max_revisions` (03-0).
- **`AccessDenied`/`HashMismatch`/`NotReadable` при чтении locator'а** —
  уже определённые `ArtifactError` варианты; 03.3 требует, чтобы каждое
  срабатывание логировалось в тот же audit/diagnostic sink, что отклонения
  contract/grant в 03.1 (`ContractError`-запись: variant, `parent_task_id`,
  `child_task_id`, `parent_sequence`, timestamp, без raw payload).
- **Reviewer запрашивает full artifact без grant** — read отклоняется как
  `ArtifactError::AccessDenied`; coordinator должен явно выдать
  `"artifact_read_full"`, повторный запрос без grant не эскалируется
  автоматически.

## Проверки

- selected context: id/locator, не входящий в `input context ids` child'а,
  недоступен даже если формально принадлежит task namespace (allowlist test);
  создание child с недоступным id отклоняется до persistence;
- offload: report с `output_data` больше threshold offload'ится, parent
  получает `ArtifactRef` вместо inline данных; report с `Privacy::Secret`/
  `Sensitive` содержимым не offload'ится вообще и не появляется в summary;
- locator access: child не может прочитать locator другого task namespace
  без grant; повторный tool call с тем же locator проверяется заново (не
  кэшируется результат первой проверки), а не только при первом чтении;
- grant drift для artifact access: parent grant сужается после создания
  child → следующее чтение locator'а отклоняется (симметрично drift-тесту
  03.1 для tool-call grants);
- reviewer: без явного `"artifact_read_full"` доступен только summary/hash/
  locator, не содержимое; выдача `"artifact_read_full"` со scope на другой
  locator не расширяет доступ за пределы этого locator;
- reviewer не может изменить код, имея доступ к diff и evidence (write tool
  call отклоняется по capability matrix независимо от artifact access);
- секрет (`Privacy::Secret`) не переходит соседнему child или role ни через
  offload, ни через summary — не появляется ни в Artifact Store, ни в
  diagnostics/trace;
- каждое отклонение (`ContextIdNotAccessible`, `AccessDenied`,
  `PrivacyForbidsOffload`, `HashMismatch`) создаёт audit-запись с variant и
  correlation ids, без raw payload.

## Критерии готовности

- child получает только явно перечисленный selected context и свой
  scratchpad — не весь task namespace, к которому у него формально есть
  доступ;
- большой или sensitive-неприемлемый результат offload'ится или блокируется
  по privacy label до появления в родительском context, без ручной
  санитизации summary как отдельного шага;
- child не расширяет права родителя и не обходит approval через artifact
  access (grant проверяется на каждом чтении, не кэшируется);
- reviewer получает summary-only доступ по умолчанию и не может изменить
  код ни при каком уровне artifact grant;
- контекст одного ребёнка не протекает в другого — locator access ограничен
  task namespace и explicit selected context одновременно;
- все отклонения доступа логируются с correlation ids и без raw payload.
