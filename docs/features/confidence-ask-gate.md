# Confidence Ask-Gate

Confidence Ask-Gate — native Core-механизм, который перед опасным tool-вызовом оценивает уверенность модели, опыт прошлых операций, статистику tools и reflection history.

## Поведение

1. Core определяет risk level по планируемым операциям.
2. Собираются доступные confidence signals.
3. При недостаточной уверенности Core создаёт approval request.
4. WinUI показывает запрос, scope и preview.
5. Решение пользователя возвращается через named pipe и записывается в approval audit.

Минимум два отсутствующих сигнала требуют явного вопроса. High-risk операции требуют подтверждения независимо от удобства UI.

## Данные

- `tool_execution_stats` — сглаженная статистика успеха;
- `confidence_audit_log` — объяснение решения;
- `confidence_settings` — thresholds;
- `approval.required` — native IPC event.

## Проверки

Core тестируется без UI. WinUI compatibility tests проверяют envelope и rendering approval state.
