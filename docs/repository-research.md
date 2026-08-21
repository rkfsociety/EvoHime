# Карта будущих планов EvoHime

Бывший сводный исследовательский документ разделён на самостоятельные
тематические заготовки. Каждый файл можно превращать в отдельный план после
сверки с текущим кодом и документацией.

## Правила работы с разделами

- Файл раздела содержит только требования, границы, зависимости и критерии
  готовности. Подробности внешних источников сюда не возвращаются.
- Перед планированием сверять код, `docs/current-state.md`,
  `docs/architecture.md`, `docs/development-plan.md` и действующие IPC/SQLite
  схемы.
- Один полноценный план должен иметь собственный scope, миграции, тесты,
  security review, критерии отката и проверяемый результат.
- UI остаётся projection/control layer. Durable state, policy, execution,
  memory и evaluation принадлежат Rust Core/SQLite.
- Блокирующая зависимость от более позднего этапа недопустима.

## Файлы и зависимости

| Файл | Раздел | Зависимости | Приоритет |
|---|---|---|---|
| 01 execution ledger | Журнал выполнения и typed receipts | перенесён в план 08 | блокирующий фундамент |
| 02 policy and capabilities | Policy, permissions, scope и approval | перенесён в план 09 | блокирующий фундамент |
| 03 IPC adapters and providers | IPC, version negotiation и provider boundary | перенесён в план 10 | блокирующий фундамент |
| 04 memory and RAG | Память, retrieval и forget | перенесён в план 11 | следующий слой |
| [05-telemetry-and-evaluation.md](repository-research/05-telemetry-and-evaluation.md) | Наблюдаемость, fixtures и evaluation | 01–03 | следующий слой |
| [06-browser-backend.md](repository-research/06-browser-backend.md) | Изолированный browser backend | 01–03 | отдельный optional-план |
| [07-voice-and-ambient-audio.md](repository-research/07-voice-and-ambient-audio.md) | Voice pipeline и ambient audio | 01–03 | отдельный optional-план |
| [08-vision-and-documents.md](repository-research/08-vision-and-documents.md) | Vision и document worker | 01–03, 05 | отдельный optional-план |
| [09-workflow-automation-and-simulation.md](repository-research/09-workflow-automation-and-simulation.md) | Длительные jobs, automation и simulation | 01–05 | поздний этап |
| [10-release-criteria-and-open-decisions.md](repository-research/10-release-criteria-and-open-decisions.md) | Общие release gates и нерешённые вопросы | все разделы | сопровождающий файл |

## Рекомендуемый порядок

```text
01 execution ledger
       ↓
02 policy/capabilities
       ↓
03 IPC/adapters/providers
       ├──→ 04 memory/RAG
       ├──→ 05 telemetry/evaluation ──→ 08 vision/documents
       ├──→ 06 browser
       └──→ 07 voice/ambient
                    04 + 05 + foundation
                              ↓
                    09 workflow/automation
```

Файл 10 используется на каждом этапе и не является самостоятельной
реализационной задачей.

Раздел 01 перенесён в подпункты плана 08; исходный исследовательский файл
удалён после переноса требований и критериев готовности.

Раздел 02 перенесён в подпункты плана 09; исходный исследовательский файл
удалён после переноса требований и критериев готовности.

Раздел 03 перенесён в подпункты плана 10; исходный исследовательский файл
удалён после переноса требований и критериев готовности.

Раздел 04 перенесён в подпункты плана 11; исходный исследовательский файл
удалён после переноса требований и критериев готовности.

## Общая архитектурная граница

```text
Electron renderer → Electron/main IPC → authenticated desktop IPC
                  → Rust Core → SQLite
                  → Windows supervisor
```

Внешний runtime, вторая база данных, публичный HTTP control plane, обход
authenticated IPC и model-generated authority над filesystem/network/secrets
не входят в базовую архитектуру.
