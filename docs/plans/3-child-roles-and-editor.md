# Подплан 3 — child roles и native workflow editor

Статус: средняя сложность
Порядок: 3 из 5
Источник: бывший единый мастер-план; актуальная детализация находится в этом подплане.

## Цель

Подключить существующие bounded child-role/handoff contracts к реальному
read-only выполнению и сделать его наблюдаемым в WinUI. Контрактный слой уже
умеет валидировать handoff, урезанный context, read-only capabilities и report,
а storage/IPC уже принимают и сохраняют описатели заявок и отчёты. Этот
подплан добавляет отсутствующий runtime dispatcher, выполнение и UI; он не
считает сохранённую заявку выполненной задачей.

## Фактическая база и границы

- `child_roles` уже содержит роли `coordinator`, `researcher`, `planner`,
  `implementer`, `reviewer`, `tester` и bounded `custom`, а также immutable
  redacted handoff contracts.
- `child_runtime` уже содержит `ChildTaskKind`: `onboarding`, `code_search`,
  `threat_model_review`, `test_plan_review`, `documentation`. В MVP этот
  allow-list закрыт. `research`, `validation`, `security_audit` и `code_review`
  не добавляются автоматически: каждый новый kind требует отдельного
  capability/profile, adapter и тестов.
- Разрешённые capabilities текущего контракта: `workspace.read`,
  `workspace.search`, `git.diff`, `git.status`. Write, shell, commit, install,
  network mutation, elevation и nested child запрещены на Core boundary.
- Текущие hard limits контракта: reduced context — до 32 элементов и 16 KiB,
  один элемент — до 2 048 символов; serialized report/output — до 32 KiB,
  report sources — до 32 элементов по 512 символов. `child_task_id` и
  `parent_task_id` должны быть уникальными в рамках родительского запуска и
  не переиспользоваться после terminal state.
- Реальный execution, timeout, cancellation, tool-count budget, dispatcher и
  child event stream пока отсутствуют. Сохранение `ChildTaskRequest` или
  `ChildReport` само по себе не является execution.

## Объём

- Core dispatcher только для текущего `ChildTaskKind` allow-list и только из
  non-child parent context;
- выдача урезанного context и уникального `child_task_id` из Core, без передачи
  полного parent prompt, секретов или неразрешённых capabilities;
- read-only execution adapters для `workspace.read`, `workspace.search`,
  `git.diff` и `git.status` с общей policy snapshot;
- отдельные filesystem/network sandbox и запрет write, shell, commit, install,
  network mutation, elevation и nested child на request, adapter и tool layers;
- единые timeout/cancellation/output limits и composite budget;
- проверка родителем report, confidence и sources до включения evidence в
  plan/build. Непринятый report не становится evidence и не меняет parent
  state;
- durable child lifecycle и redacted provenance: request, state transitions,
  terminal reason, report hash, sources и acceptance decision;
- versioned IPC events/replay/reconnect для child timeline;
- WinUI catalog, descriptor editor, timeline, evidence panel и
  blocked/error states.

## Policy defaults для MVP

Значения должны попасть в Core-owned immutable policy snapshot, чтобы UI не мог
ослабить их после запуска. Это стартовые defaults, а не обещание навсегда:

- timeout: 5 минут на child; hard maximum — 15 минут;
- `max_output_bytes`: default 16 KiB, hard maximum текущего контракта — 32 KiB;
- reduced context: hard maximum текущего контракта — 16 KiB;
- tool budget: максимум 32 read-only tool calls на child; каждая операция
  дополнительно ограничивается своим filesystem/search/git лимитом;
- composite budget считается по wall-clock, числу tool calls и output bytes;
  превышение любого компонента немедленно переводит child в terminal
  `budget_exceeded`. Model-token budget учитывается только если provider
  возвращает измерение; его отсутствие не снимает wall-clock/tool/output
  limits;
- report confidence хранится как integer `0..100`. Автоматическое принятие
  evidence разрешено только при `confidence_percent >= 70`, непустом summary,
  валидных уникальных sources и статусе `complete` или `partial`. Диапазон
  `0..69`, `rejected`, отсутствующие/невалидные sources или secret-like content
  дают `blocked`/`rejected`, но не acceptance. Порог должен быть policy
  snapshot и виден в evidence UI.

## Lifecycle и ошибки

Состояния child: `queued` → `running` → `waiting_approval` либо `cancelling` →
один terminal state из `completed`, `partial`, `blocked`, `failed`,
`cancelled`, `timed_out`, `budget_exceeded`, `output_exceeded`.

- превышение output останавливает adapter, не обрезает молча report и не
  публикует частичный evidence;
- timeout отменяет активную операцию и фиксирует `timed_out`; поздний report
  отклоняется;
- cancellation идемпотентна: повторная отмена не меняет terminal state;
- forbidden capability, nested child, invalid descriptor или invalid report
  дают `blocked`/`rejected` с machine-readable reason и redacted detail;
- runtime/provider/filesystem failure даёт `failed`, без blind retry. Retry
  возможен только как новый child с новым id и новым policy snapshot;
- parent acceptance gate атомарно проверяет request/report pair, task id,
  status, bounds, confidence, sources и provenance до записи acceptance.

## Replay и reconnect

Каждое child event получает обычный монотонный Core `sequence_id`, durable
`child_task_id` и детерминированный event kind. При reconnect UI запрашивает
replay после последнего sequence; Core возвращает журнал в порядке sequence,
а UI дедуплицирует по `(child_task_id, sequence_id)` и восстанавливает snapshot
состояния. Terminal state хранится durable и является конечным: отменённый,
timed-out или failed child не может быть переведён поздним report обратно в
running/completed. Если событие уже было в журнале до reconnect, повторная
доставка не создаёт второй timeline item.

## Workflow editor и UI MVP

- Catalog показывает только разрешённые kinds, их read-only capabilities,
  limits, expected report shape и доступные роли.
- Descriptor editor — форма, а не произвольный script runner: parent task,
  kind, role, reduced-context items, read-only capability preview и budget
  preview. Write/shell/network mutation/nested child нельзя выбрать или
  скрыть в JSON.
- Editor показывает локальную схему `parent → child`, но drag-and-drop и
  свободный граф не являются обязательными для MVP. Child запускается явной
  командой с immutable preview; дополнительные зависимости появятся только
  вместе с task graph contract.
- Timeline — линейный список переходов с timestamp, sequence, duration,
  terminal reason и reconnect-safe status; graph view не требуется для
  первого vertical slice.
- Evidence panel показывает summary/findings/sources, confidence, policy
  threshold, provenance и решение parent gate. Непринятое evidence визуально
  отделено от принятого и не попадает в plan/build context.
- Для `waiting_approval`, `blocked`, `failed`, `cancelled`, `timed_out`,
  `budget_exceeded` и `output_exceeded` UI показывает понятную причину и
  доступное действие; UI не подменяет Core state локальным успехом.

## Порядок реализации

1. Зафиксировать Core-owned child policy snapshot, allow-list и lifecycle
   state machine поверх существующих contracts/storage.
2. Ввести Core dispatcher и read-only execution adapters с единым
   timeout/cancellation/output/tool budget.
3. Реализовать parent acceptance gate, source validation, confidence policy и
   evidence provenance.
4. Добавить durable child events, IPC compatibility, replay/reconnect и
   terminal-state conflict rules.
5. Добавить native catalog/descriptor editor, timeline/evidence views,
   blocked/error states и visual smoke.
6. Провести focused contract, adapter, cancellation/timeout/budget,
   replay/reconnect, acceptance-gate и WinUI smoke tests.

## Критерии готовности

- child не может получить elevated permissions, создать child или выполнить
  mutation tool; запрет проверяется на Core boundary и повторно перед adapter;
- все текущие пять `ChildTaskKind` либо реально исполняются read-only
  adapter-ом, либо явно показываются как `not_implemented`, но не создают
  ложный `completed`;
- timeout, cancel, forbidden capability, invalid report, oversized output и
  любой budget limit приводят к bounded terminal state с причиной;
- parent принимает только валидный report с policy threshold confidence,
  непустым summary, уникальными валидными sources и matching provenance;
- UI одинаково и truthful показывает queued, running, waiting approval,
  blocked, failed, cancelled, timed out, budget exceeded и completed/partial;
- replay после reconnect не дублирует child events и не воскрешает terminal
  child;
- focused Rust tests, IPC compatibility tests, WinUI smoke и `git diff --check`
  проходят; generated artifacts очищены.

## Зависимости

- `child_roles`, `child_runtime`, child storage и базовые IPC commands уже
  существуют как контрактный/persistence слой, но их execution wiring ещё не
  готов — это основной объём данного подплана;
- task lifecycle, event journal, cancellation, replay/reconnect и bounded
  checkpoint/recovery foundation уже существуют в Core, однако child-specific
  lifecycle/events/lease integration ещё нужно добавить;
- подплан 4 не является обязательной зависимостью для security boundary или
  focused testability. Его `RunPolicy`/runner можно переиспользовать позже,
  но MVP подплана 3 должен иметь самостоятельный Core-owned child policy,
  deterministic adapters и тестовый harness;
- полноценные pause/resume, arbitrary dependency graph и provider/model
  routing остаются в подплане 4 и не должны быть скрыты внутри child editor.
