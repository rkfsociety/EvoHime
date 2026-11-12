# План 192.0 — Agent-facing доступ к Persistent Analysis Kernel

Статус: active implementation contract. Исходной issue нет: направление выведено
из аудита checkout после вопроса «для чего нужна вкладка Анализ». План не
означает, что перечисленное уже исправлено.

## Цель

Сделать Persistent Analysis Kernel (план 28, закрыт 30.08.2026) реально
используемым: агент получает объявленный в реестре инструментов ограниченный
доступ к уже существующей Core-owned kernel runtime, а вкладка «Анализ»
показывает ядра, созданные во время задачи. Никакой второй runtime, второй
реестр, второй policy-путь или перенос authority в renderer не создаются.

## Основание checkout

- Kernel contract и исполнение уже есть: `crates/evohime-core/src/analysis_kernel.rs`
  (`KernelRuntime`, `KernelHostRequestV1`, `put_ephemeral_object`, `admit`,
  `execute`), storage `crates/evohime-local-storage/src/analysis_kernel.rs`
  (schema v38: sessions/objects/events/idempotency), worker
  `crates/evohime-supervisor/src/analysis_kernel_worker.rs`.
- Поддерживаются только `json_parse`, `json_select`, `csv_summary`,
  `object_put`; `filesystem`, `network`, `shell`, `credentials` отклоняются
  typed error до эффекта. Inline-аргументы с чувствительными маркерами
  отклоняются (`contains_sensitive_marker`).
- `KernelRuntime::execute` и IPC dispatch уже возвращают bounded
  `KernelHostResponseV1`; в SQLite сохраняется только metadata.
- Процессные ядра живут в `IpcBridge`:
  `crates/evohime-core/src/ipc_bridge_header.rs:656`
  (`analysis_kernels: Arc<Mutex<HashMap<..KernelRuntime>>>`), то есть недоступны
  агенту, который работает в coordinator (`ToolAgent.tools: Arc<ToolRegistry>`).
- Kernel не объявлен ни одним инструментом: в
  `crates/tool-runtime/src/registry.rs` нет ни одного `analysis.*` id, в
  `crates/tool-runtime/src/manifest.rs` нет схемы, в
  `crates/evohime-core/src/adaptive_tool_catalog.rs` нет метаданных, поэтому
  проекция каталога не может его отобрать.
- Вкладка `Анализ` (`desktop/evohime-electron/src/renderer/src/AnalysisKernelPanel.tsx:12-33`)
  только фильтрует `event.analysisKernel` из потока событий. Проекция
  присылается лишь в ответ на `createAnalysisKernel`/`getAnalysisKernel`,
  кнопок нет, `listAnalysisKernels` не существует — пустое состояние
  недостижимо из UI.
- Прецедент подмены исполнения в Core:
  `crates/evohime-core/src/core_agent_execution.rs:1375` отдельно обрабатывает
  `memory.search`, а `crates/tool-runtime/src/tools/memory.rs:23` остаётся
  заглушкой. Это тот же дублирующий dispatch, который план 191.1 удаляет,
  поэтому копировать его нельзя.
- Авторизованный snapshot инструментов строится в
  `crates/evohime-core/src/core_agent_execution.rs:190-214`, probe-аргументы —
  `catalog_preflight_input` (`core_prelude.rs:546`), fail-closed пустого
  snapshot — `253-254`.
- Ожидаемое число инструментов в Doctor — литерал `32`
  (`ipc_bridge_header.rs:37`, использование
  `ipc_bridge_workspace_operations.rs:24`); дефект уже входит в план 191.1.
- `recover_analysis_kernels` (`core_journal_core.rs:141`) fencing-ит running
  сессии в `Crashed` на рестарте Core и останавливает worker.

## Граница

Core остаётся владельцем сессий, лимитов, идемпотентности и admission;
supervisor — владельцем процесса worker; `tool-runtime` — владельцем
декларации инструмента и его input schema. Renderer по-прежнему получает
только projection: id, task/workspace, runtime version, хеши, status, revision,
object count, error code. Значения объектов и inline result в UI не передаются.

Агент работает только через объявленный инструмент. Он не может:
- выбрать исполняемый файл, окружение, cwd или версию runtime;
- перейти на `filesystem`/`network`/`shell`/`credentials` операции;
- прочитать или записать произвольный artifact;
- расширить лимиты задачи;
- получить доступ к чужому ядру (сессия привязана к task id).

## Изменяемые контракты

- Новые builtin-инструменты `analysis.*` в декларации `tool-runtime` с
  явными input schemas, без прав на файловую систему и сеть.
- Необязательная kernel-host зависимость самого `ToolRegistry`
  (`with_kernel_host`), чтобы dispatch оставался один и не потребовал
  изменения 70 мест конструирования `ToolContext`.
- Общий Core-owned kernel host, который используют и агент, и IPC-команды
  `CreateAnalysisKernel`/`ExecuteAnalysisKernel`/`ResetAnalysisKernel`.
- Additive IPC-команда списка ядер задачи и соответствующая проекция.
- Детерминированные `task_id`/`workspace_id`/`package_manifest_hash`/
  `policy_hash` для kernel-сессий, создаваемых агентом: только вычисленные
  Core значения, никогда не модель.

## Recovery и rollback

Неизменяемость сохраняется: инструмент не даёт authority, approval-путь не
меняется, `PermissionEngine` остаётся единственным источником решения по
правам. Kernel остаётся bounded вычислительной средой: при отказе worker,
превышении лимита или крахе процесса инструмент возвращает typed error
(`kernel_unavailable`, `limit_exceeded`, `forbidden_capability`), а не
retry-цикл. После рестарта Core running-сессии fencing-ятся в `Crashed`; агент
не повторяет запрос вслепую и обязан создать новое ядро явным шагом.

Rollback: удаление объявления `analysis.*` из каталога возвращает прежнее
поведение, потому что у агента просто нет инструмента. Исторические записи
`analysis_kernel_*` не удаляются, схема не мигрирует. Отсутствие
`kernel_host` в `ToolRegistry` даёт typed `unavailable`, а не fallback на
in-process исполнение в обход worker.

## Этапы

- [Этап 1 — декларация инструментов и host-контракт](./192-1-core-analysis-kernel-agent-access.md)
- [Этап 2 — Core runtime-интеграция и recovery](./192-2-core-analysis-kernel-agent-access.md)
- [Этап 3 — IPC-проекция и вкладка «Анализ»](./192-3-core-analysis-kernel-agent-access.md)
- [Этап 4 — verification, release evidence и закрытие](./192-4-core-analysis-kernel-agent-access.md)

## Зависимости

### Блокирующие

- Закрытый план 28: kernel contract, storage v38, worker launch contract
  `trusted-local-1` и его Job Object лимиты не переопределяются.
- `ToolRegistry`/`ToolManifest`/`ToolManifest::validate` как единственный
  владелец декларации и валидации схем.
- `PermissionEngine` и durable approval как единственный путь авторизации.
- План 191.1 в части «одна декларация вместо дублирующего dispatch»: если
  191.1 ещё не закрыт, этап 192.1 добавляет инструмент в текущую
  `registry.rs`, а не заводит параллельную таблицу.

### Опциональные

- План 191.2 (отбор и промпт-профиль): kernel-инструменты должны попадать в
  курируемый fallback и в RU/EN-отбор, но отсутствие 191.2 не блокирует этапы
  192.1–192.2.
- Публикация результата в `ArtifactStore` вместо inline-ответа при превышении
  bounded inline размера — полезное расширение, не обязательное для этапов 1–2.
- План 184 может использовать provenance kernel-вызовов как вход trace
  evaluation.

## Критерии готовности

- [x] Этап 192.1: агент видит `analysis.run` в авторизованном snapshot;
      вызов без Core-owned host даёт typed `analysis_kernel_unavailable`.
- [ ] Агент получает осмысленный typed результат
      `json_parse`/`json_select`/`csv_summary` (этап 192.2).
- [ ] Ни один вызов kernel не проходит мимо `ToolRegistry`: в
      `core_agent_execution.rs` не появляется новый `if call.name == ...`.
- [ ] Инструмент не даёт файловых, сетевых, shell- или credential-возможностей;
      запрещённые операции возвращают typed error до эффекта.
- [ ] Ядро создаётся и переиспользуется в пределах задачи, привязано к task id
      и не переживает потерю процесса без явного reset.
- [ ] Значения объектов, inline result и абсолютные пути не попадают в UI,
      trace и release evidence; фиксируются только id, операция, хеши и размеры.
- [ ] Вкладка «Анализ» показывает существующие ядра задачи после рестарта UI.
- [ ] Отсутствующий host или worker даёт typed `unavailable`, а не скрытый
      fallback.
- [ ] Static и focused test evidence приложены, canonical docs обновлены только
      после подтверждённого поведения.

## Non-goals

- Новые kernel-операции сверх `json_parse`/`json_select`/`csv_summary`/
  `object_put`.
- Сетевой, файловый или credential-доступ из kernel.
- Дать агенту право создавать ядро с произвольными лимитами или runtime
  identity.
- Семантический выбор инструмента как обязательный путь.
- Перенос kernel runtime, storage или admission в renderer либо в tool-runtime.
- Изменение схемы SQLite: план опирается на существующие v38-таблицы.
- Повышение release-версий сверх правила `AGENTS.md` о фактически затронутых
  публикуемых модулях.

## Verification

Плановые gates: contract/schema tests для объявленных инструментов →
runtime-интеграция с worker/recovery и focused Core tests → IPC/Electron
contract tests и projection bounds → static verification, focused tests и
Windows acceptance. Проверки выполняются при реализации соответствующего этапа
и не выдаются за выполненные заранее.

## Release evidence

Хранить: id и версии объявленных `analysis.*` инструментов, хеш декларации,
результат проверки схем, mapping «инструмент → kernel-операция», bounded
диагностику отбора, focused test/CI evidence, подтверждение отсутствия
изменений схемы SQLite. Не сохранять значения объектов, inline result, raw
prompts, secrets и абсолютные пути.

## Критерии выхода overview

- [ ] Ownership, backward compatibility и failure semantics согласованы до
      этапа 1.
- [ ] Каждый этап проверяет observed-поведение; ни один не создаёт второй
      kernel runtime, второй реестр инструментов или альтернативную
      policy-проверку.
- [ ] Зависимости разделены на блокирующие и опциональные, блокирующих ссылок
      на более поздние этапы нет.
