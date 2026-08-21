# План 06-4 — Evaluation, security и закрытие плана

## Цель

Доказать, что workflow orchestration пригодна для поставляемой Евы, и после
полного выполнения перенести контракт в каноническую документацию и удалить
временные файлы плана.

## Зависимости

### Блокирующие

- [06-3](06-3-workflow-desktop.md);
- deterministic evaluation catalog и security smoke gates;
- полный набор Rust, Electron, IPC и packaging checks из `AGENTS.md`.

### Опциональные

- дополнительные локальные модели и embeddings. Их отсутствие не блокирует
  закрытие: обязательные fixtures используют deterministic provider/fallback.

## Evaluation matrix

1. Валидный sequential workflow выполняется в ожидаемом порядке.
2. Diamond graph даёт bounded fan-out и deterministic fan-in.
3. `AND` ждёт все ветки, `OR` принимает только объявленные route events.
4. Child output schema и provenance проверяются до fan-in.
5. Retry не повторяет non-retryable error и не превышает budget.
6. Approval, cancellation, timeout и restart дают правильные terminal states.
7. Crash до/после dispatch marker не приводит к blind retry.
8. Renderer получает projection, но не raw prompt, secret, unrestricted child
   context или произвольный tool result.
9. MCP server identity, transport, tool name и capability grants не могут быть
   подменены model output или входом renderer; untrusted stdio/remote server
   отклоняется до запуска.
10. Попытки cycle, route injection, grant escalation, nested child, path escape,
    unbounded loop и oversized payload отклоняются до эффекта.
11. Старые IPC-клиенты продолжают работать с additive workflow protocol.

## Документация и закрытие

- перенести подтверждённый workflow contract и runtime invariants в
  `docs/architecture.md`;
- перенести фактическое состояние, шаблоны и результаты проверок в
  `docs/current-state.md`;
- обновить `docs/features/task-dependency-graphs.md`, чтобы он ссылался на
  канонический contract и не описывал неподключённое поведение;
- проверить внутренние ссылки, `git diff --check`, generated protocol и
  отсутствие упоминаний AutoGen как runtime-зависимости;
- удалить `docs/plans/06-*.md` только после полного acceptance и task-only
  commit.

## Готово, когда

Все обязательные eval/security/packaging проверки зелёные, current-state и
architecture отражают код, план удалён по правилам репозитория, а опубликованный
commit можно воспроизвести на `main`.
