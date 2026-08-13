# Reflection loop

ReflectionStage выполняется в Rust Core после observation каждого tool-вызова.

Он оценивает успех, ищет повторяющиеся failure patterns в experience memory, сохраняет reflection event и добавляет короткую подсказку в следующий model context. Три последовательных ошибки переводят задачу в `revise_plan`; retry ограничен budget и не снимает approval gate.

## События

- `agent.reflection` — versioned task event;
- `task.failed` — финальная ошибка после исчерпания recovery;
- `task.completed` — успешное завершение.

Переключатель: `EVOHIME_REFLECTION_ENABLED=0`.
