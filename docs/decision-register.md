# EvoHime — реестр зависимостей и решений

Канонический register завершённых решений desktop-цикла 17–19. Здесь нет секретов, provider credentials или
неподтверждённых обещаний поставки. Статус `accepted` означает решение,
зафиксированное кодом и текущей архитектурой; `open` означает, что до release
нужна отдельная проверка или интеграция.

## Dependency graph

| План | Блокирующие зависимости | Опциональные зависимости и fallback | Evidence / владелец |
| --- | --- | --- | --- |
| 07 | 01–06 | optional tool adapters; fail-closed unsupported fallback | Product/Core architecture |
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
| 19 | 17, existing updater and authenticated Core startup | optional PR API; fallback is explicit push to configured product branch | Repair/update owner |

Граф линейный для основного runtime: `01–06 → 07 → 08 → 09 → 10 → 11 → 12 →
16 → 17`. Планы 13–15 реализуют optional adapters, подключаются через
fail-closed boundaries и не образуют цикл или обязательную зависимость базового
пакета.

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
| D-SIGN-01 | Authenticode signing is outside the current release scope; manifest/hash is the documented trust root | Release | `docs/architecture.md`, `docs/release-audit.md` |
| D-REPAIR-01 | Self-repair is user-triggered only; diagnosis, commit, push and restart are separate approvals, and repair never edits the selected workspace | Repair/update | `docs/architecture.md`, `docs/current-state.md`, Electron repair tests |
| D-UPDATE-01 | Installed package keeps its backup until the relaunched shell authenticates Core and writes bounded health-marker; timeout rolls back | Repair/update | `crates/evohime-updater`, health-marker tests, `docs/release-evidence.md` |

## Decision closure register

| Decision ID | Status | Owner | Closure criterion | Release impact |
| --- | --- | --- | --- | --- |
| O-AUTO-01 | accepted | Core automation | Scheduler timezone/missed-tick, durable cursor, additive IPC and focused gates are wired | closed by plan 18.1 evidence |
| O-AUTO-02 | accepted | Core automation | Archive/restore transaction, checksum, bounded restore and retention sweep are covered by focused evidence | closed by plan 18.2 evidence |
| O-LIC-01 | accepted | Release | Locked Cargo/npm metadata inventory and hash verification pass the CI gate | closed by plan 18.3 evidence |
| O-REPAIR-01 | accepted | Repair/update | Isolated user-triggered repair, protected paths, separate commit/push/CI gates and health-gated rollback pass focused checks | closed by plan 19.0 evidence |

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
