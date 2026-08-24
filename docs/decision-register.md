# EvoHime — реестр зависимостей и решений

Канонический register плана 17.1. Здесь нет секретов, provider credentials или
неподтверждённых обещаний поставки. Статус `accepted` означает решение,
зафиксированное кодом и текущей архитектурой; `open` означает, что до release
нужна отдельная проверка или интеграция.

## Dependency graph

| План | Блокирующие зависимости | Опциональные зависимости и fallback | Evidence / владелец |
| --- | --- | --- | --- |
| 07 | отдельное направление, не блокирует 16–17 | — | Product/Core architecture |
| 08 | 01–06 | — | Core + SQLite ledger tests |
| 09 | 08 | — | Core policy/approval tests |
| 10 | 08–09 | — | desktop IPC contract tests |
| 11 | 08–10 | local embeddings; fallback FTS5 | Core RAG fixtures |
| 12 | 08–11 | advisory judge; deterministic gate | eval gate scripts |
| 13 | 10 | browser backend; typed `backend_unavailable` | Core adapter owner |
| 14 | 10, 12 | voice/ambient backend; privacy-safe unsupported | Core listener owner |
| 15 | 10, 12 | vision/document backend; typed `backend_unavailable` | Core adapter owner |
| 16 | 08–12, existing workflow contracts | 13–15; automation remains Core-only without adapters | Core automation fixtures A01–A08 |
| 17 | 07–16 current-state/architecture contracts | external backend never becomes blocking | Release owner |

Граф линейный для исполняемых этапов: `08 → 09 → 10 → 11 → 12 → 16 → 17`;
планы 13–15 подключаются только как optional adapters и не образуют цикл.

## Accepted decisions

| Decision ID | Решение | Владелец | Evidence |
| --- | --- | --- | --- |
| D-IPC-01 | Core — единственный executor и source of truth; renderer получает только projection | Core | `docs/architecture.md`, authenticated named-pipe tests |
| D-SQL-01 | SQLite schema changes are additive/transactional, backup precedes blocking migration, owner is `evohime-local-storage` | Storage | `LocalDatabase::migrate`, backup tests |
| D-AUTO-01 | Automation does not reuse workflow lease ownership; automation uses its own fenced runtime and durable events | Core automation | `automation_runtime.rs`, `automation_store.rs` |
| D-AUTO-02 | Simulation admits only fake-provider effects; host effects fail closed | Core automation | `automation_simulation.rs`, A06 |
| D-OPT-01 | Missing browser/voice/vision adapter is typed unsupported and has no production side effect | Capability owner | architecture/current-state optional adapter sections |
| D-RES-01 | Base package is local-only: no cloud control plane, public HTTP, external telemetry backend or mandatory GPU | Release | `AGENTS.md`, architecture boundaries |
| D-LIC-01 | License/attribution inventory is a checked-in metadata document, never runtime input or secret storage | Release | `docs/licenses/` when third-party material is shipped |

## Open decisions

| Decision ID | Status | Owner | Closure criterion | Release impact |
| --- | --- | --- | --- | --- |
| O-AUTO-01 | open | Core automation | Wire scheduler timezone/missed-tick and additive automation IPC; rerun A01/A08 on clean package | blocks automation release-green claim |
| O-AUTO-02 | open | Core automation | Add archive/restore transaction and retention sweep evidence beyond contract fixtures | blocks archive release gate |
| O-LIC-01 | open | Release | Populate `docs/licenses/` for every distributed third-party artifact and verify hashes | blocks final installer audit |
| O-SIGN-01 | open | Release | Provide real signing pipeline/certificate evidence; until then manifest/hash remains the documented trust root | blocks signed-release claim, not local dev |

## Resource and contract budgets

| Resource | Bound | Enforcement owner |
| --- | --- | --- |
| Automation input | 64 KiB | Core contract |
| Automation activities | 64 | Core contract |
| Automation command queue | 256 pending | Core runtime |
| Automation progress | 1024 coalesced entries | Core runtime |
| Provider call | 120 s deadline, at most 2 retry attempts | Core runtime |
| Snapshot | 1 MiB, 64 per run | Core simulation/storage |
| Durable history | 256 events per run | Core acceptance |
| Archive | 10,000 runs / 30 days | Release gate |
| Simulation | fake provider only; no host/network/process/IPC | Core simulation |

Каждое изменение schema или IPC обязано обновить owner, version, migration,
rollback note и focused compatibility test в том же task-only коммите.
