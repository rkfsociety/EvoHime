# Карта переноса направлений EvoHime

Это архивная карта происхождения завершённых планов, а не список текущих
задач. Все перечисленные направления сверены с кодом и перенесены в
канонические документы; новые работы ведутся через
[`development-plan.md`](development-plan.md) и отдельные файлы в
[`plans/`](plans/).

| Исторический раздел | Текущее направление | Канонический контракт |
| --- | --- | --- |
| Execution ledger и typed receipts | 08 | [`architecture.md`](architecture.md), [`current-state.md`](current-state.md) |
| Policy, capabilities и approval | 09 | [`architecture.md`](architecture.md), [`current-state.md`](current-state.md) |
| IPC adapters и provider boundary | 10 | [`architecture.md`](architecture.md), [`current-state.md`](current-state.md) |
| Typed memory и Core-first RAG | 11 | [`architecture.md`](architecture.md), [`current-state.md`](current-state.md) |
| Telemetry и deterministic evaluation | 12 | [`evaluations.md`](evaluations.md), [`current-state.md`](current-state.md) |
| Изолированный browser backend | 13 | [`architecture.md`](architecture.md), [`current-state.md`](current-state.md) |
| Voice и ambient audio | 14 | [`architecture.md`](architecture.md), [`current-state.md`](current-state.md) |
| Vision и document worker | 15 | [`architecture.md`](architecture.md), [`current-state.md`](current-state.md) |
| Workflow automation и simulation | 16 | [`architecture.md`](architecture.md), [`current-state.md`](current-state.md) |
| Release gates и open decisions | 17 | [`decision-register.md`](decision-register.md), [`release-audit.md`](release-audit.md) |

## Архитектурная граница

```text
Electron renderer → Electron/main IPC → authenticated desktop IPC
                  → Rust Core → SQLite
                  → Windows supervisor
```

Durable state, policy, execution, memory и evaluation принадлежат Rust Core и
SQLite. Renderer остаётся projection/control layer. Внешний runtime, вторая
база данных, public HTTP control plane и model-generated authority над
filesystem/network/secrets не входят в базовую архитектуру.
