# EvoHime — доменный контракт: Model Gateway, профили и маршрутизация

Канонический контракт домена провайдеров, ключей, маршрутизации моделей, каскадов и стратегий промптов. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).
См. также: [../providers/](../providers/), [../features/extended-reasoning.md](../features/extended-reasoning.md).

## Model gateway и routing

Model Resilience Policy v1 — отдельный Core-owned слой после выбора primary
route. Он закрепляет policy snapshot/hash, bounded retry/fallback budgets,
нормализованные классы ошибок, compatibility по capability/privacy/data
residency и cancellation. Fallback принимает только заранее разрешённые
`ModelProfileRef`; provider payload пересобирается внутри gateway adapter и
credentials между providers не переносятся. Policy/run overlay ephemeral,
поэтому после restart внешний вызов не повторяется: существующий model
provenance ledger переводит dispatch в `interrupted` или `unknown_outcome`.
IPC 188/event 43 отдаёт только bounded metadata; renderer не получает prompt,
output или provider payload.

Классификация provider-отказов учитывает HTTP status, встроенный adapter-ом в
текст ошибки (`"<status>: message"`). Отказ ключа (`401`/`403`), исчерпанный
баланс (`402`) и неизвестная модель (`404`) — клиентские ошибки: Core не
тратит retry budget и не открывает circuit, потому что повтор не может
завершиться успехом. Повторяемыми остаются `429`, timeout, сетевой сбой и
`5xx`; `403` с явным rate-limit маркером классифицируется как cooldown. Тот же
контракт использует и gateway fallback loop, и Core resilience policy, поэтому
обе стадии принимают одинаковое решение. В trace и в `task.failed` передаётся
bounded `error_code` (`provider_auth_rejected`, `provider_quota_exhausted`,
`provider_model_not_found`, `provider_rate_limited`, `provider_timeout`,
`provider_unreachable`, `provider_unavailable`, `provider_request_rejected`,
`provider_configuration`), а renderer показывает по нему понятную причину:
provider text, URL и ключ в projection не попадают.

Core является владельцем model gateway и принимает routing-решения после
проверки privacy, offline, approval/tool policy, capability, health, evaluation
gate и budget. User preference передаётся только как hint внутри разрешённого
множества; renderer не выбирает provider и не может обойти Core policy.

Целевой контракт плана 02 состоит из четырёх последовательных этапов:

1. provider contract — capability metadata, immutable policy snapshot, health и
   bounded retry/circuit contract;
2. local provider — loopback-only модель под lifecycle и Job Object supervisor;
3. routing и budget — подключение selection/execution к agent run, fallback,
   versioned redacted trace и terminal result;
4. UI — read-only проекция фактического route и trace-состояния через desktop
   IPC.

Все этапы сохраняют tool permissions, approval requirements, sandbox и privacy
границы при fallback. Trace и enum-контракт для UI версионируются; malformed
payload или несовместимая major-версия не интерпретируются renderer частично.
Реализация этапа 02 завершена. Snapshot/overlay selector, loopback local
provider, supervisor-owned adapter lifecycle в Job Object, authenticated
Core↔supervisor command channel, bounded fallback/approval workflow,
versioned runtime catalog, redacted replayable trace и typed routing UI
подключены к agent loop. Временные планы этапа удалены после переноса
контракта сюда и подтверждённого состояния в [`current-state.md`](../current-state.md).

Conversation-bound trace передаёт projection v3 с bounded metadata для
non-terminal и terminal событий: тип/фаза, безопасные имена инструментов,
tool telemetry, token/candidate counters, routing status и размеры скрытого
содержимого. Для `task.failed` Core экспортирует `error_code`, `source` и
`operation` из allow-list токенов либо детерминированно классифицирует ошибку.
При отказе filesystem boundary дополнительно экспортируются только
`path_form`, `path_scope` и `path_boundary_reason`; исходный путь не пересекает
projection boundary. URL, prompt, arguments, tool output, секреты и полный
текст ошибки не пересекают projection boundary. Даже malformed или oversized
payload получает bounded deterministic fallback fields.
Та же failure projection применяется до записи durable conversation history:
`assistant_message_failed` и `task_failed` больше не сохраняют исходный
`error`, поэтому повторное чтение истории не может вернуть raw provider text.
До вставки в основной durable event journal `task.failed` получает ту же
projection: глобальный replay хранит только `task_id`, `error_code`, `source`,
`operation` и безопасные path-boundary tokens при их наличии, а
transcript/recovery UI не использует raw `error` даже для legacy/live payload.
Trace UI показывает базовые поля failure отдельной диагностической карточкой;
при filesystem boundary failure он также показывает безопасные path tokens и
повторяет их в Markdown-разделе `diagnostics`, поэтому failure остаётся
читаемым и после сохранения файла. Renderer дополнительно нормализует legacy `task.failed`
payload перед показом и экспортом, поэтому старое или ошибочно доставленное
событие не возвращает URL, prompt или секрет в trace surface.
Обычные payloads renderer сначала рекурсивно редактирует, а экспорт добавляет
deterministic summary: диапазон и непрерывность sequence, counts по типам,
итог задачи, tool started/output/telemetry, bounded `tool_failures` и routing statuses. Main bridge
отклоняет необработанные URL и чувствительные assignment-поля до native save
dialog.
Читаемая часть экспорта строится из Core-owned журнала conversation
(`core.getConversationEvents`), а не из redacted trace projection: renderer
собирает раздел `transcript:` и секцию «Транскрипт беседы» с промптом, текстом
ассистента, вызовами инструментов, их выводом и терминальной ошибкой — теми же
renderer-payload'ами, что показывает чат. Чтобы вызовы инструментов были
понятны, `CoreEvent::ToolStarted` и conversation-событие `tool_started` несут
bounded guardrail-redacted `arguments` (≤ 8 KiB с суффиксом `…[truncated]`,
`<sensitive_data_blocked>` при блокировке guardrail) — они проходят тот же
guardrail, что и сам вызов инструмента, и ограничены, чтобы не выбить событие из
bounded conversation journal. `CoreEvent::ToolOutput` по той же причине несёт не
больше 32 KiB текста с суффиксом
`…[tool output truncated; request the rest with a smaller limit or offset]`:
без этого ограничения один большой вывод инструмента не помещался в
conversation payload 64 KiB, и событие `tool.output` заменялось маркером
`conversation.projection_failed`, теряя результат вызова из транскрипта.
Egress-guardrail сканирует исходящий текст не одним вызовом с лимитом 64 KiB,
а bounded-чанками по границам строк, поэтому большой tool output или сообщение
пользователя редактируются целиком, а не отвергаются по размеру. Отказ
guardrail сохраняет стабильный код в `task.failed`: `sensitive_data_blocked`
для найденного секрета, `egress_payload_too_large` для строки больше
chunk-лимита и `egress_guardrail_unavailable` для иной ошибки; причина также
пишется в model-trace событием `model.egress_blocked` без тела сообщения.
Промпт, arguments и tool output при этом
по-прежнему не пересекают projection boundary живого потока и остаются доступны
только через conversation log; shell-граница `asTraceContent` маскирует URL и
секретные assignment-поля перед записью файла.
Shell-owned Ollama fallback не маскируется под Core event: main broadcast
передаёт в trace только allow-listed `shell.ollama_download_fallback` с typed
`error_code/source/operation`, а trace явно различает `observed=yes/no`.

## Ключ провайдера

Ключ вводится в `ProviderForm` и остаётся в main-процессе. Значение шифруется ОС через Electron `safeStorage` (DPAPI на Windows) и сохраняется в `provider.json`; LiteRouter и OpenAI Responses имеют отдельные записи, а каждый вендор OpenAI-compatible транспорта — свой профиль с собственным ключом, endpoint, моделью и настройками. Renderer получает только summary с признаком `configured` и без секретов. Core собирает model gateway из окружения при старте, поэтому сохранение настроек перезапускает supervisor вместе с Core, а pipe client переподключается к новой сессии. В окружение попадают только переменные выбранного профиля, чтобы ключ другого провайдера не дошёл до gateway. Для каждой модели профиль хранит независимые RPM/RPH/RPD rolling-window лимиты; новый лимит выключен по умолчанию, ноль обозначает отсутствие соответствующего cap, учёт выполняется Core отдельно для точного model ID. Codex CLI не является записью в этом списке: его ChatGPT-аутентификация принадлежит локальному CLI. Если ОС отказывается шифровать, ключ не записывается вовсе. Пауза между запросами отображается для LiteRouter и при явно сохранённом нестандартном значении, чтобы настройку можно было отменить.

Headless Linux CLI `eva` перед автозапуском Core читает allow-list настроек из
`$XDG_CONFIG_HOME/evohime/provider.env` или `~/.config/evohime/provider.env`.
Ключ передаётся Core через унаследованное окружение процесса; каталог и файл
хранятся локально в текстовом виде с правами `0700` и `0600`. Значения, явно
заданные в окружении `eva`, имеют приоритет над файлом.

В композере выбирается ровно один источник для следующей задачи: активный API-профиль
(LiteRouter, OpenAI Compatible, OpenAI Responses или Ollama) либо `codex_cli`. Для API
выбор активного профиля перезапускает supervisor/Core с его изолированными
credentials; выбор уже настроенного профиля в настройках автоматически
сохраняется и сразу применяет профиль через этот перезапуск; каталог моделей
рядом с композером запрашивается у выбранного провайдера. Выбор Codex передаёт отдельный IPC intent, Core принимает Codex только
при явном этом режиме и запускает bounded `codex exec` в каноническом workspace.
Electron распознаёт штатную WinGet-установку в `%LOCALAPPDATA%\OpenAI\Codex\bin\<version>\codex.exe`,
фиксированный legacy-путь и executable из `PATH`; ответ WinGet об отсутствии
более новой версии не считается ошибкой, если CLI уже найден.
Выбор самой API-модели передаётся в Core для следующего запроса без перезапуска.
Модель Codex сохраняется в `shell/codex.json` и после выбора перезапускает
supervisor/Core, чтобы следующий запуск `codex exec` получил новую модель.
Обычные dialogue-задачи не меняют backend. Codex stdout/stderr приходят как bounded `tool.output`, а
отсутствующий CLI, пустая модель, отмена и ненулевой exit дают terminal failure
без silent fallback.

Base URL принимается только по `https` либо по `http` на loopback: ключ отправляется на этот адрес, и произвольный http-хост означал бы его утечку.

## Model request provenance v1

Каждый Core model request имеет versioned canonical envelope из
`contracts/model-request/v1/`, связь с единственным `context_ledger.id`,
durable blocks/sources и lifecycle status. `EventJournal` предоставляет Core
API `commit_model_request`, `mark_model_dispatch`, startup recovery и bounded
retention; renderer не строит и не изменяет envelope.

Новые записи проходят `FullForDispatch`; canonical hash использует JCS и
domain-separated SHA-256. Retry/fallback создают отдельный request attempt с
`parent_request_id`/`previous_request_hash`. Source capture, shadowing,
responses/tool intents и typed tombstones хранятся в Core-owned SQLite.

Offline bundle формата `evohime-provenance-export-v1` имеет allow-listed
замкнутые секции и проверяется командой `evohime-verify provenance --bundle`.

Хеш снимка gateway фиксируется до dispatch и вычисляется по реальному снимку
политики маршрутизации. В receipts ссылка на action может быть пустой, а
идентификатор request хранится отдельно; `BEGIN IMMEDIATE` сериализует
конкурирующие добавления в цепочку. Tool intents связаны с terminal effect
receipts, а redaction сохраняет неизменяемое обязательство envelope и создаёт
типизированные tombstone.

Повторная детерминированная оценка snapshot маршрутизации использует bounded
LRU-кэш с ключом из запроса, snapshot, поколения health overlay, каталога,
попытки и точного времени оценки; обновление overlay автоматически меняет
ключ и не допускает устаревшего выбора.

## Architect-Editor Model Pipeline (план 91)

Core хранит versioned pipeline с отдельными Architect/Editor ModelProfile,
typed EditIntent и exact workspace revision fence. Intent является предложением,
а не capability grant; editor writes проходят существующую Core mutation
boundary. Payload bounded: 128 путей/операций, 256 KiB intent и 3 retries.
Electron получает только redacted projection и Core-checked actions через
additive IPC; hidden reasoning и credentials не сохраняются.

## Model Edit Protocol Registry v1 (план 97)

Core хранит versioned SEARCH/REPLACE, bounded patch, structured JSON и
whole-file протоколы с `model_profile_id`. Каждое определение требует SHA-256
precondition; parse и dry-run выполняются до effect, stale/ambiguous edits
отклоняются fail-closed. Repair feedback ограничен тремя попытками. Реальная
запись делегируется approved `filesystem_write`/`filesystem_patch` через
Revision-Safe Workspace Files, а IPC 245/event 90 и Electron показывают только
redacted metadata.

## Prompt Cache Planner v1 (план 105)

Core строит deterministic `PromptSegment` список с content hash, stable/dynamic
флагом, exact context/policy revisions и profile-driven provider capabilities.
Cache key не меняет instruction или security semantics; secret segments
отклоняются. Keepalive выключен по умолчанию и ограничен bounded profile policy.
Measured metrics хранятся отдельно от raw prompts; authenticated IPC 247/event
92 и Electron показывают только metadata.

## Model Purpose Routing v1 (план 115)

`ModelPurposeRoutingPolicy` — Core-owned versioned registry из 13 typed
`ModelCallPurpose`. Каждый route задаёт registered profile ref, required
capabilities/privacy, tool ceiling и context policy; canonical hash фиксирует
policy snapshot. Валидированная policy хранится аддитивно в SQLite с
optimistic version fence, а при model call загружается Core и применяется к
следующему вызову.

Purpose routing формирует только primary route/profile hint и ограничивает
tools; `Model Resilience Policy` остаётся единственным владельцем retry и
fallback. Model-call trace/provenance содержит purpose и policy hash, raw
prompts, credentials и hidden reasoning не входят в policy или renderer
projection. Authenticated IPC command 256/event 101 и Electron panel дают
bounded get/put surface; invalid policy, stale version и unsafe
tool/context combination отклоняются до эффекта.

## Free Provider Reliability Routing v1 (план 125, реализован 2026-09-09)

Core contract хранит provider identity, scoped free access state и bounded
reliability snapshot; Model Gateway остаётся transport/retry authority.
Недостаток свежего evidence даёт `UnknownNeedsRefresh`/`Unknown`, sparse samples
не становятся Healthy, а credentials и raw provider payloads не сохраняются.
SQLite schema v100 добавляет только reliability metadata.

## Cloud Provider Profiles contract v1 (план 173, закрыт 2026-09-25)

`ProviderProfileId` является явной стабильной identity для `openai`, built-in
OpenAI-compatible профилей (включая AnyModel) и `custom`; она независима от
`ProviderKind` транспорта. Новые настройки оболочки сохраняют ID профиля
отдельно от base URL. Маршруты старого формата без ID остаются совместимыми;
только точные ранее известные endpoint могут восстановить legacy identity,
произвольный endpoint остаётся generic OpenAI-compatible. Фиксированные
профили строят URL из trusted defaults. Для Cloudflare account ID принимается
отдельно, проверяется как 32 шестнадцатеричных символа и используется только
для построения account-scoped URL; host/path не редактируются. Токен остаётся в
зашифрованном provider store и передаётся Core через окружение supervisor.

Core `ProviderProfile` и `ProviderModelDescriptor` сохраняют версию,
profile/catalog revisions и content hashes, typed limits, capability
provenance, privacy, usage и lifecycle. Они адаптируют канонический gateway
`ModelCatalogEntry`; неподтверждённые capability/privacy/usage остаются
`Unknown`. Raw provider response, credential material, произвольный endpoint
из catalog и headers в descriptor не включаются. SQLite schema v174
транзакционно хранит bounded snapshots со scope provider/credential-binding/
region, monotonic revision fence, content hash, lifecycle и typed failure;
validated recovery гидратирует только согласованные записи в ограниченный
process-local cache.

Model Gateway остаётся единственным владельцем provider transport и запроса.
Совместимые профили используют общий OpenAI-compatible transport и bounded
`/models` discovery; Cloudflare использует тот же account ID и отдельный
bounded Model Search запрос с pagination, page/entry/body/time caps. Каталоги
нормализуются детерминированно. Состояния `Fresh`, `Stale`, `Unavailable`,
`CredentialRejected` и `DiscoveryUnsupported` имеют отдельные typed outcomes;
404 запроса списка моделей означает отсутствие discovery endpoint, а не
отсутствие выбранной модели. `DiscoveryUnsupported` остаётся видимым и не
блокирует вручную выбранный model ID после проверки настроенного route и
credential. Известный stale/expired/failed catalog и подтверждённое отсутствие
модели блокируют dispatch по существующей route policy.

Core-owned route preflight проверяет модель непосредственно перед dispatch.
Только capability `Unsupported` с provenance `ProviderDeclared` или `Observed`
блокирует chat или требуемые запросом tool calls; `Unknown` не выдаётся за
поддержку и не блокирует запрос без policy-требования доказательства. Renderer
получает только bounded redacted `provider_catalog` в существующем
authenticated `model.catalog` событии, показывает профиль, lifecycle,
capability и model selection, но не читает storage/API, не определяет vendor и
не принимает routing/privacy решения. Скрытые credentials и raw provider
ошибки не попадают в UI.

Начальное provider health в route snapshot — `Unknown`, пока нет probe или
наблюдения. Существующие `RunHealthOverlay`, circuit breaker, route ranking и
redacted `routing.trace` остаются владельцами наблюдённых runtime-сбоев;
unknown-route health отображается как «состояние не проверено». Свежий каталог
сам по себе не доказывает доступность провайдера. План не добавляет постоянный
health store, обязательные live probes, billing/payment или автоматическую
активацию провайдера.
`routing.trace` schema v2 кодирует `Unknown` отдельно от `Healthy`; Electron
читает ранее сохранённые v1 traces и отвергает неизвестные enum values в v1.

## Empirical Free-Access Verification v1 (план 174, закрыт 2026-09-25)

`FreeAccessEvidence` принадлежит Core и дополняет advisory `FreeAccessState`.
Evidence отделяет advertised и observed access, activation, allowance,
provenance, limits/units, successful samples, confidence, expiry, invalidation
и canonical content hash. Каждый snapshot привязан к opaque per-key credential
binding и provider-profile revision/hash; при ротации ключа evidence прежнего
binding удаляется транзакционно до hydration, а ошибка удаления останавливает
startup. SQLite schema v172 хранит bounded snapshot и monotonic revision для
provider/model/credential/region scope. Core валидирует запись, hash и scope
при чтении и гидратирует только текущие configured profile scopes.

Core-owned coordinator поддерживает `Disabled` по умолчанию, consent-gated
automatic policies и явную одноразовую ручную проверку. Manual и automatic
probes делят single-flight limiter, profile-scoped cooldown, bounded synthetic
request (8 output tokens, zero retries, 45-second deadline) и shutdown
cancellation; in-flight work не восстанавливается после restart. Renderer
получает только redacted projection через authenticated provider IPC/events.
В settings UI manual verification доступна для настроенного OpenRouter profile
с явным предупреждением о возможной оплате. Для нового OpenRouter profile UI
предвыбирает автоматическую проверку при первом использовании; она не начнётся,
пока пользователь отдельно не подтвердит возможную оплату. Ранее сохранённый
выбор профиля сохраняется. Persistent automatic consent сбрасывается при замене
или удалении ключа.

Единственный authority, который может подтвердить `VerifiedFreeLimited`, —
фиксированный OpenRouter model-detail pricing response: запрошенная model
identity должна совпасть, а все обязательные и объявленные charge dimensions
должны быть exact zero. Только после semantic completion и валидного usage Core
создаёт strict-free evidence. Cloudflare Workers AI probes ограничены явным
cost consent и могут сохранять typed activation/quota observations; bounded
whitelist parser извлекает только numeric error codes 5035/3036 с точного
Cloudflare endpoint. 5035 означает `ActivationRequired`, 3036 — `QuotaRejected`.
Успешный Cloudflare completion остаётся `Unknown`, пока не появится
authoritative pricing adapter. Остальные профили также остаются `Unknown` без
собственного authority. Raw provider bodies, prompts, headers и credentials не
сохраняются.

`FreeOnly` проверяет свежесть, profile hash, provenance и strict eligibility
на Core route preflight; stale, trial, activation-required, paid, quota-invalidated
и unknown evidence блокируют dispatch. `PreferFree` использует отдельный
explicit paid-fallback option. Catalog `free` label и `:free` suffix остаются
advisory. Надёжность, качество и eligibility остаются разными сигналами;
resolved route snapshot не меняется во время одного stream.

## Confidence-Gated Model Cascade v1 (план 129, MVP реализован 2026-09-09)

Core contract `confidence_gated_model_cascade` задаёт bounded policy с
threshold/revision/hash и fail-closed decisions `Unavailable`, `NeedsReview`,
`Eligible` или `Denied`. Model Gateway остаётся единственным routing owner;
без versioned confidence producer или eligible route cascade не исполняется.

### Core-owned Prompt Strategy Profiles v1

`crates/evohime-core/src/prompt_strategy.rs` owns immutable, versioned strategy profiles, bindings, lifecycle/evidence checks, deterministic selection and replay snapshots. SQLite metadata is additive in schema v179; profile, example-set, output-contract and selection rows are immutable, while lifecycle transitions use revision checks. Registry rows retain hashes and artifact references, not reusable prompt text. Few-shot assets must resolve to validated privacy-approved artifacts.

The Core resolves a profile against the exact model route capability declaration, context/loadout/output constraints and fresh redacted holdout evidence. Synthetic routing defaults do not qualify as positive capability evidence. Missing/corrupt historical profile revisions fail closed. Each dispatch records a route-specific snapshot with hashes of prepared messages and effective tool schemas, correlated to model provenance. Decomposition and MultiSample are sequential, tool-free, pinned to one local/offline/free route, bounded by aggregate input/output token budgets and sent with provider output limits and retries disabled. Other strategies compose through the existing context builder and tool/output contracts.

Authenticated Core operations expose bounded list/get/evidence/compatibility/compare and explicit registration, binding, promotion and lifecycle transitions. Guided workflow runs recover their exact strategy pin; the Electron Agents settings surface shows bounded metadata only. Generic `TaskStarted` and `ModelContext` events retain their compatible fields but carry omission markers instead of user/system prompts. CI acceptance and module publication evidence are tracked in `release-evidence.md`.
