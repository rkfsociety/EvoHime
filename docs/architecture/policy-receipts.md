# EvoHime — доменный контракт: политики, approval gate, guardrails и signed receipts

Канонический контракт домена политик исполнения, approval gate, guardrails и криптографических квитанций. Обновлено: 2026-11-18.
Остов архитектуры: [../architecture.md](../architecture.md).
См. также: [../security/](../security/), [../../SECURITY.md](../../SECURITY.md).

## Signed receipts

Canonical Receipt v1 реализован в `crates/evohime-receipts` и Electron main
consumer `desktop/evohime-electron/src/main/receipt-crypto.ts`. Нормативные
JCS bytes, envelope `receipt_hash`, Ed25519, result domain, schema, limits,
stable error codes и cross-language vectors находятся в
`contracts/receipts/v1/`; подробное правило — `docs/security/receipt-canonical-v1.md`.
Этап 01.1 фиксирует bytes и проверку контракта. Key lifecycle реализован в
`crates/evohime-receipts`: Windows DPAPI CurrentUser and owner-only DACL;
Linux owner-only directory/file modes,
SQLite-источник переходов и audit, journaled rotation/recovery, explicit
trusted genesis, signed checkpoint contract и `evohime-verify.exe`. Core
публикует renderer только bounded status/key metadata; private material не
выходит из Core. JSONL history является post-commit snapshot с manifest и
статусом stale при ошибке экспорта. Runtime orchestration 01.3 теперь
выполняется Core-owned `ReceiptRuntime`: mutation path использует durable
`pre_action` до dispatch, approval хранится как bounded one-shot intent, а
terminal post/refusal append-ятся в SQLite hash-chain. Startup recovery
устанавливает guard, истекает старые approval intents и переводит незакрытые
вызовы в `pending_recovery`; raw input/result в receipt runtime не сохраняются.
Для восстановления результата Core предоставляет authenticated
`ReconcilePendingReceiptAction`: он создаёт новый read-only action с собственным
hash/call binding и атомарно связывает его с историческим action; исходный tool
повторно не запускается. `ClosePendingReceiptAction` закрывает только explicit
unknown-result как signed refusal, а authenticated `UnquarantineReceiptAction`
проверяет trusted signed checkpoint и закрывает только invariant violation как
refusal. Protected recovery rows шифруются
AES-256-GCM, а storage-key rotation выполняется возобновляемыми bounded batch с
durable cursor. Read-only sampling, recovery state и bounded runtime counters
доступны только через Core diagnostics.
Retention/compaction receipt chain по-прежнему относится к отдельному этапу
01.4.

## Policy, capability snapshots и approval gate

План 09 реализован поверх `ReceiptRuntime`, без второго execution runtime или
отдельного approval store. `evohime_receipts::capability::CapabilitySnapshotV1`
создаётся для действия как bounded immutable contract: он содержит session/run/
task identity, policy и manifest hashes, operation/tool scopes, network и adapter
references, opaque secret purposes и budgets. Snapshot получает
domain-separated canonical `snapshot_hash`; child snapshot допускается только как
subset родителя. Raw secret values, prompt и необрезанный tool input в snapshot
не попадают.

`crates/evohime-core/src/policy_gate.rs` предоставляет единственный Core-owned
`preflight`/`recheck_before_effect` gate. Перед effect он повторно сверяет
canonical call hash, normalized scope, tool identity, snapshot hash и policy
version. Terminal IPC, обычный ToolAgent и workflow adapter проходят этот gate;
прямой production-вызов Tool Runtime после approval использует только
`execute_after_durable_approval` после atomic durable claim. Старый in-memory
approval wrapper существует только в unit-test compatibility code.

SQLite receipts дополнены additive таблицами
`receipt_capability_snapshots` и `receipt_policy_decisions`, а также nullable
session/snapshot/policy/hook linkage columns в action и approval intent. Это
сохраняет чтение старых баз и позволяет durable различать `allowed`,
`approval_required`, `denied`, `unavailable`, `expired`, `cancelled`,
`policy_error` и `unknown_outcome`.

Approval rejection записывается как terminal durable state и не может быть
повторно повышен до grant. Claim проверяет session, snapshot и policy version;
изменение canonical call, scope или policy делает approval stale. Bounded
preflight/postflight hooks получают только action/tool/input/snapshot hashes и
typed outcome metadata. Acceptance покрывает отказ до side effect, повторную
доставку, drift, redaction и recovery; старые receipt decision values остаются
читаемыми для миграции.

## Standing Approval Profiles v1 (план 87)

Core хранит versioned scoped profiles (schema 1, storage 85) с bounded rules,
subject/action/resource/risk/foreground matching, expiry и use limits. Critical
risk всегда hard-deny; match возвращает объяснимую причину и не расширяет
capabilities. Additive authenticated IPC tags 239/84 дают list/create/update/
revoke/match, а каждое решение требует дальнейшей проверки ExecutionPolicy.

## Approval Policy Profiles v1 (план 88)

Отдельный Core-owned policy layer (schema 1, storage 86) детерминированно
решает `require_prompt` по scope/action/resource/risk с expiry. Critical risk
всегда остаётся hard requirement; unknown/missing rule требует prompt. IPC
tags 240/85 предоставляют list/create/update/revoke/decide. Этот слой не
выдаёт capabilities и не отменяет ExecutionPolicy.

## Sensitive Data Guardrails v1

`sensitive_data_guardrails.rs` — Core-owned bounded detector/redactor на
contract version 1. Правила имеют deterministic SHA-256 policy snapshot и
precedence `block > hash > mask > redact`; detectors покрывают email,
secret/bearer tokens и private keys. Structured JSON обходится рекурсивно с
лимитами depth 16/nodes 512, stream держит bounded carry и обрабатывает
совпадения между chunks. Текстовый input больше 64 KiB сканируется bounded
чанками по границам строк, поэтому большой tool output или model message
редактируется целиком; одна строка сверх лимита, неизвестный и структурно
oversized/malformed input по-прежнему fail closed.

Admission применяется к provider model messages, tool input/output, streaming
API и `model-trace.jsonl`. Local authoritative records отделены от redacted
provider/renderer projection; tool permissions, approval и effect ledger не
ослабляются. Policy и stream state process-local, schema v43 не изменяется.

Authenticated additive IPC использует `SensitiveDataGuardrailsCommand` tag 186
и `SensitiveDataGuardrailsEvent` tag 41 для bounded `status`/`evaluate`.
Electron показывает только destination, policy hash, action/count/status;
raw input/output, credentials и rule bodies в renderer не попадают.

Ошибки HTTP-провайдера диагностируются отдельным bounded-контрактом
`ProviderErrorDiagnostic`: adapter читает не более 16 KiB тела ответа и
извлекает только `type`, `code`, `param`, `message`, безопасный request ID и
число HTTP-попыток. Core trace связывает событие также с номером попытки,
model-attempt ID, выбранной моделью и purpose вызова.
Полное тело, заголовки авторизации и URL в trace не сохраняются. Перед записью
в `model-trace.jsonl` текстовые поля проходят Core redaction, а URL и типовые
формы credential-полей удаляются отдельным sanitizer; trace
дополнительно фиксирует размер/обрезание тела и факт JSON-декодирования. Это
позволяет отличить, например, неизвестную модель, неверный параметр и лимит
контекста, не превращая provider response в источник утечки.

## Execution Policy Profiles v1

`evohime-tool-runtime::execution_policy_profiles` — единственный resolver для
зарегистрированных `shell_execute` и `process_run`. Versioned profile содержит
explicit network/environment policy, bounded timeout/output, mandatory process
tree cleanup и backend requirement; command text и user `env` не выбирают
policy. Environment наследуется только через scrubbed allow-list.

До `spawn` общий validator для обоих инструментов отклоняет известные прямые
сетевые клиенты/туннели и URL `*.trycloudflare.com`; аргументы, одновременно
указывающие на `.env*`, SSH/AWS/Solana credentials, keystore или сигнатуры
секретов и на upload/POST, также отклоняются.
Это bounded защита от известных command-shaped стилеров, а не OS-level network
изоляция: произвольный разрешённый executable может иметь собственный сетевой
код. Поэтому профильное `NetworkPolicy::Deny` само по себе не следует считать
принудительно применённым firewall; неизвестные способы исходящего трафика
остаются отдельной задачей плана 181.

На Windows restricted backend использует Job Object с `KILL_ON_JOB_CLOSE`;
timeout/cancel/drop очищает дерево. Required backend failure происходит до
dispatch success. На других системах разрешён только portable bounded режим
без заявления OS-level sandbox guarantee. Canonical JSON profile hash и
storage schema v44 хранят validated catalog, version/hash/json; handles,
output, leases и cancellation state ephemeral.

Authenticated additive IPC — command tag 187/event tag 42. Electron получает
только profile id/version/hash, backend, policies и bounds через metadata-only
панель; raw command, environment, credentials и process output не пересекают
boundary.

## Output Guardrail Pipeline v1 (план 84)

Core-owned pipeline выполняет bounded Validate/Transform/Redact stages перед
accepted output/handoff. Schema 83 и IPC tags 237/82 сохраняют только typed
metadata; correction attempts ограничены тремя, а Sensitive Data Guardrails
остаётся canonical redaction policy.

## Policy-Aware Tool Result Cache v1 (план 113)

Core-owned cache result contract разрешает только trusted metadata для
read-only tools; default `Never`, mutating/network tools и tools с secret refs
не кэшируются в MVP. Ключ детерминированно включает schema/tool version,
input hash, resource scope, authority scope и policy hash. Entries содержат
только result/provenance refs, observed time и TTL; `RequireFresh` всегда
обходит cache, а drift и invalidation делают entry непригодной.

SQLite schema v89 хранит bounded versioned entries с optimistic fence; eviction
ограничивает cache 512 записями. IPC command 254/event 99 и Electron panel
передают только redacted metadata. Cache не создаёт capability, не заменяет
effect execution и не содержит raw credentials или raw tool result.
