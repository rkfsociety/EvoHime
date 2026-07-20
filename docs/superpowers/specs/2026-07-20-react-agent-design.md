# Native ReAct Agent Design

## Цель

Перевести агентный runtime EvoHime с planner-first схемы на полноценный native tool-calling ReAct loop. Модель после каждого действия получает observation и сама выбирает следующий tool call, финальный ответ или остановку.

Пользовательский интерфейс показывает только безопасные статусы, вызовы и результаты инструментов. Скрытая цепочка рассуждений модели не сохраняется и не передаётся в браузер.

## Принятые решения

- Использовать вариант 2: полностью заменить обязательный `collect_plan_steps` и bounded replan на native tool-calling loop.
- Не выводить chain-of-thought. Показывать статусы вроде «Модель выбирает действие», `tool.started`, `tool.output`, `tool.completed`, approval и финальный ответ.
- Сохранить существующие permissions, approvals, cancellation, checkpoints, recovery и memory admission/feedback.
- `memory.search` остаётся обычным инструментом ReAct. Предварительно retrieved memory продолжает передаваться как дополнительный untrusted context.
- Один ответ модели может содержать несколько вызовов; параллельное выполнение разрешено только для явно независимых вызовов и не должно обходить permission checks.
- Опасные операции не получают автоматического разрешения от ReAct-контроллера.

## Архитектура

```text
User message
  -> ReAct controller
  -> LLM: history + project/rules context + memory context + tool schemas
  -> assistant tool_call(s)
  -> permission / approval check
  -> tool execution
  -> tool result as observation
  -> LLM chooses next action
       |-> another tool_call
       |-> assistant.reply
       `-> stop/error
```

Новый controller должен быть отдельным модулем внутри `crates/agent-runtime/src/agent_loop/`, чтобы orchestration loop не смешивался с нормализацией tool inputs, лимитами и финальным ответом. Существующие tool registry и model gateway используются через native OpenAI-compatible tool calling.

## Состояние ReAct-итерации

Каждая итерация имеет:

- порядковый номер;
- assistant message с native tool calls или финальным reply;
- имя инструмента и структурированные аргументы;
- approval state, если операция защищённая;
- observation: structured result, human-readable output, success/error;
- число повторных попыток для конкретного tool call;
- usage/telemetry metadata.

Внутреннее rationale модели не является полем состояния и не попадает в протокол.

## Жизненный цикл

1. Собрать system rules, project index context, историю и предварительно retrieved memory.
2. Добавить schemas всех разрешённых инструментов и `assistant.reply`.
3. Вызвать модель через native tool-calling API.
4. Если модель вернула `assistant.reply`, завершить задачу и отправить финальный текст.
5. Если модель вернула tool calls, проверить каждый вызов через существующий permission engine.
6. При необходимости поставить задачу на approval wait и сохранить checkpoint.
7. Выполнить разрешённые вызовы последовательно или независимым батчем.
8. Добавить результаты как tool observations в историю модели.
9. Повторить с шага 3 до финального ответа, лимита или ошибки.

Ошибки инструментов передаются модели как observations с кодом ошибки, retryability и кратким сообщением. Повтор допускается только в пределах лимита и не должен повторять идентичный безуспешный вызов бесконечно.

## Лимиты и защита

Контроллер обязан иметь конфигурируемые значения с безопасными defaults:

- максимальное число итераций;
- максимальное общее число tool calls;
- максимальный token/result budget истории;
- общий timeout задачи;
- timeout отдельного вызова инструмента;
- ограничение retry для одинакового вызова;
- обнаружение повторяющейся пары `tool + arguments` без изменения observation.

При исчерпании лимита задача завершается контролируемым сообщением об остановке, а не падением сервера. Уже выполненные мутации не откатываются автоматически.

## Checkpoints и recovery

Checkpoint должен хранить достаточно данных для продолжения после approval или рестарта:

- ReAct iteration;
- сериализованную модельную историю с assistant/tool messages;
- pending tool calls и их approval state;
- завершённые observations;
- counters/limits;
- used memory ids;
- pause/error reason.

Resume продолжает с последнего состояния и не выполняет повторно tool call, для которого observation уже сохранён. Невыполненный pending call проходит permission check повторно.

## События и UI

Используются существующие события `tool.started`, `tool.output`, `tool.completed`, `approval.required`, task status и `agent.message.delta`. При необходимости добавляется безопасное событие статуса фазы без rationale, например `agent.status` со значениями `selecting_action`, `executing_tools`, `waiting_approval`, `responding`.

Браузер не получает raw prompt, скрытые рассуждения или внутренний decision trace. Он получает имя инструмента, безопасные аргументы по существующим правилам редактирования, output и статус выполнения.

## Память и RAG

Память не переписывается. Перед первым вызовом controller получает:

- bounded retrieved memory как untrusted system context;
- planner suggestions только если они уже поддерживаются текущим retrieval API, переименованные в action hints и не являющиеся командами;
- `memory.search` в tool catalog для on-demand lookup.

После завершения задачи остаются текущие extraction, admission gate, `memory.ask`, used-memory attribution и feedback/decay. Tool observations от `memory.search` не должны автоматически становиться памятью без существующего post-task extraction flow.

## Ошибки и безопасность

- Native tool call с неизвестным именем отклоняется как model/tool protocol error.
- Невалидные аргументы не передаются в runtime; модель получает observation с validation error.
- Permission denial возвращается модели как non-retryable observation, если пользователь явно не изменил решение.
- Approval pause не блокирует остальные независимые безопасные вызовы, если модельный ответ допускает их разделение и checkpoint это поддерживает.
- Cancellation останавливает текущий tool execution и дальнейшие LLM calls через существующие cancellation tokens.
- Tool output и observations ограничиваются существующим result budget перед следующей отправкой модели.

## Тестирование

Нужны тесты на:

- последовательный цикл `tool call -> observation -> tool call -> assistant.reply`;
- несколько независимых tool calls в одном model response;
- остановку по `assistant.reply` без лишнего tool call;
- invalid tool name и invalid arguments;
- retryable/non-retryable tool errors;
- повторный одинаковый вызов и iteration/tool-call/timeout limits;
- permission denial и approval pause/resume;
- checkpoint/recovery без повторного выполнения завершённого вызова;
- cancellation во время tool execution и между LLM iterations;
- ограничение tool outputs и сохранение memory used ids;
- subagent depth/budget и запрет бесконечного fan-out;
- protocol/UI regression: безопасные статусы и отсутствие rationale/chain-of-thought.

## Не входит в эту работу

- вывод полного reasoning или chain-of-thought;
- новый универсальный vector RAG по всему репозиторию;
- изменение схемы memory items и алгоритма embeddings;
- автоматическое обходное разрешение protected tools;
- новый визуальный дизайн Memory/Chat panels;
- обучение или fine-tuning модели.

## Критерий готовности

Для запроса, требующего нескольких действий, модель может динамически менять следующий tool call на основании предыдущего observation, завершать работу через `assistant.reply`, восстанавливаться после approval/restart и не зацикливаться сверх заданных лимитов. Все существующие permission, cancellation, memory feedback и task lifecycle тесты продолжают проходить.
